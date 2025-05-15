use crate::api::ApiEvent;
use crate::event_loop::EventLoop;
use ed25519_dalek::SigningKey;
use libp2p::{gossipsub, identify, request_response, identity::ed25519, identity::Keypair, kad, noise, ping, swarm::NetworkBehaviour, tcp, yamux, Multiaddr, Swarm, SwarmBuilder};
use libp2p::request_response::{Behaviour as RequestResponseBehaviour, Config as RequestResponseConfig, ProtocolSupport};
use local_ip_address::local_ip;
use sha2::{Digest, Sha256};
use std::time::Duration;
use std::{error::Error, hash::{DefaultHasher, Hash, Hasher}};
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use std::io;
use async_trait::async_trait;
use bincode::{Decode, Encode};
use libp2p::request_response::Codec;
use futures::prelude::*;
use crate::dag::TxHash;

#[derive(NetworkBehaviour)]
pub struct LemuriaBehaviour {
    ping: ping::Behaviour,
    pub gossipsub: gossipsub::Behaviour,
    identify: identify::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub rr: RequestResponseBehaviour<LemuriaCodec>,
}

pub async fn new(secret_key_seed: &str, port: String) -> Result<(EventLoop, mpsc::Sender<ApiEvent>), Box<dyn Error>> {
    let keypair = if secret_key_seed.trim().is_empty() {
        Keypair::generate_ed25519()
    } else {
        generate_keypair_from_seed(secret_key_seed)
    };
    println!("Generated keypair: {:?}", keypair.public());

    let mut swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
        .with_dns()?
        .with_behaviour(|key| create_behaviour(&key).expect("Failed to create behaviour"))?
        .build();

    println!("Peer ID: {:?}", swarm.local_peer_id());

    swarm.behaviour_mut().kademlia.set_mode(Some(kad::Mode::Server));

    // // Listen on localhost
    // let local_listen_addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port).parse().expect("Failed to parse local multiaddr");
    // Swarm::listen_on(&mut swarm, local_listen_addr.clone()).expect("Failed to start listening on local network");
    // swarm.add_external_address(local_listen_addr.clone());

    // Listen on LAN
    let ip = local_ip().expect("Failed to get local ip address");
    let lan_listen_addr: Multiaddr = format!("/ip4/{}/tcp/{}", ip, port).parse().expect("Failed to parse lan multiaddr");
    Swarm::listen_on(&mut swarm, lan_listen_addr.clone()).expect("Failed to start listening on lan network");
    swarm.add_external_address(lan_listen_addr.clone());

    println!("Node external addresses {:?}", swarm.external_addresses().cloned().collect::<Vec<_>>());

    // Subscribe to gossipsub topic
    let topic = gossipsub::IdentTopic::new("lemuria-zk-dag");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    let (tx, rx) = mpsc::channel(100);

    Ok((EventLoop::new(swarm, rx), tx))

}

pub fn generate_keypair_from_seed(seed: &str) -> Keypair {
    // Hash the seed to get 32 bytes
    let hash = Sha256::digest(seed.as_bytes());
    let seed_bytes: [u8; 32] = hash
        .as_slice()
        .try_into()
        .expect("SHA256 hash must be exactly 32 bytes");

    // Create SigningKey
    let signing_key = SigningKey::from_bytes(&seed_bytes);

    // Convert to libp2p-compatible SecretKey
    let secret = ed25519::SecretKey::try_from_bytes(&mut signing_key.to_bytes().to_vec())
        .expect("Failed to convert to libp2p SecretKey");

    // Promote to libp2p ed25519::Keypair
    let keypair = ed25519::Keypair::from(secret);

    // Wrap in libp2p::identity::Keypair
    Keypair::from(keypair)
}

fn create_behaviour(key: &Keypair) ->Result<LemuriaBehaviour, tokio::io::Error> {
    // Ping
    let ping = ping::Behaviour::default();

    // To content-address message, we can take the hash of message and use it as an ID.
    let message_id_fn = |message: &gossipsub::Message| {
        let mut s = DefaultHasher::new();
        message.data.hash(&mut s);
        gossipsub::MessageId::from(s.finish().to_string())
    };

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
        .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
        .message_id_fn(message_id_fn) // content-address messages. No two messages of the same content will be propagated.
        .build()
        .map_err(|msg| tokio::io::Error::new(tokio::io::ErrorKind::Other, msg))?; // Temporary hack because `build` does not return a proper `std::error::Error`.

    // build a gossipsub network behaviour
    let gossipsub = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(key.clone()),
        gossipsub_config,
    )
        .map_err(|e| tokio::io::Error::new(tokio::io::ErrorKind::Other, e))?;

    let mut cfg = kad::Config::default();
    cfg.set_query_timeout(Duration::from_secs(5 * 60));
    let store = kad::store::MemoryStore::new(key.public().to_peer_id());
    let kademlia = kad::Behaviour::with_config(key.public().to_peer_id(), store, cfg);

    let identify = identify::Behaviour::new(identify::Config::new("/lemuria/id/1.0.0".into(), key.public()));

    let protocols = std::iter::once((LemuriaProtocol, ProtocolSupport::Full));
    let rr = RequestResponseBehaviour::<LemuriaCodec>::new(protocols, RequestResponseConfig::default());

    Ok(LemuriaBehaviour { ping, gossipsub, identify, kademlia, rr})
}
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct DagSyncRequest { }

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct DagSyncResponse {
    pub dag_data: Vec<u8>,
    pub tips_children_data: Vec<u8>,
}

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub enum LemuriaRequest {
    DagSync(DagSyncRequest),
    // Add more request types here...
}

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub enum LemuriaResponse {
    DagSync(DagSyncResponse),
    // Add more response types here...
}

#[derive(Debug, Clone)]
pub struct LemuriaProtocol;

impl AsRef<str> for LemuriaProtocol {
    fn as_ref(&self) -> &str {
        "/lemuria/rr/1"
    }
}

#[derive(Clone, Default)]
pub struct LemuriaCodec;

#[async_trait]
impl Codec for LemuriaCodec {
    type Protocol = LemuriaProtocol;
    type Request = LemuriaRequest;
    type Response = LemuriaResponse;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where T: AsyncRead + Unpin + Send {
        let mut buf = Vec::new();
        io.read_to_end(&mut buf).await?;
        let (msg, _) = bincode::decode_from_slice(&buf, bincode::config::standard())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(msg)
    }

    async fn read_response<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Response>
    where T: AsyncRead + Unpin + Send {
        let mut buf = Vec::new();
        io.read_to_end(&mut buf).await?;
        let (msg, _) = bincode::decode_from_slice(&buf, bincode::config::standard())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(msg)
    }

    async fn write_request<T>(&mut self, _: &Self::Protocol, io: &mut T, request: Self::Request) -> io::Result<()>
    where T: AsyncWrite + Unpin + Send {
        let bytes = bincode::encode_to_vec(&request, bincode::config::standard())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        io.write_all(&bytes).await
    }

    async fn write_response<T>(&mut self, _: &Self::Protocol, io: &mut T, response: Self::Response) -> io::Result<()>
    where T: AsyncWrite + Unpin + Send {
        let bytes = bincode::encode_to_vec(&response, bincode::config::standard())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        io.write_all(&bytes).await
    }
}
