use crate::api::ApiEvent;
use crate::event_loop::EventLoop;
use libp2p::{gossipsub, identify, identity, identity::Keypair, kad, noise, ping, swarm::NetworkBehaviour, tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder};
use local_ip_address::local_ip;
use std::time::Duration;
use std::{error::Error, hash::{DefaultHasher, Hash, Hasher}};
use libp2p::multiaddr::Protocol;
use tokio::sync::mpsc;

#[derive(NetworkBehaviour)]
pub struct LemuriaBehaviour {
    ping: ping::Behaviour,
    pub gossipsub: gossipsub::Behaviour,
    identify: identify::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,

}

pub async fn new(keypair_seed: String, port: String) -> Result<(EventLoop, mpsc::Sender<ApiEvent>), Box<dyn Error>> {
    if let Ok(id_keys) = generate_keys(keypair_seed) {
        let public_key = id_keys.public();
        let mut swarm = SwarmBuilder::with_existing_identity(id_keys)
            .with_tokio()
            .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
            .with_dns()?
            .with_behaviour(|key| create_behaviour(&key).expect("Failed to create behaviour"))?
            .build();

        swarm.behaviour_mut().kademlia.set_mode(Some(kad::Mode::Server));

        let local_listen_addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port).parse().expect("Failed to parse local multiaddr");
        let ip = local_ip().expect("Failed to get local ip address");
        let lan_listen_addr: Multiaddr = format!("/ip4/{}/tcp/{}", ip, port).parse().expect("Failed to parse lan multiaddr");

        Swarm::listen_on(&mut swarm, local_listen_addr.clone()).expect("Failed to start listening on local network");
        swarm.add_external_address(local_listen_addr.clone());
        Swarm::listen_on(&mut swarm, lan_listen_addr.clone()).expect("Failed to start listening on lan network");
        swarm.add_external_address(lan_listen_addr.clone());

        println!("Node external addresses {:?}", swarm.external_addresses().cloned().collect::<Vec<_>>());

        // Subscribe to gossipsub topic
        let topic = gossipsub::IdentTopic::new("lemuria-zk-dag");
        swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

        let (tx, rx) = mpsc::channel(100);

        Ok((EventLoop::new(swarm, rx), tx))

    } else {
        eprintln!("Keypair generation failed");
        Err("Keypair generation failed".into())
    }
}

fn generate_keys(keypair_seed: String) -> Result<Keypair,Box<dyn Error>> {
    if keypair_seed.is_empty() {
        let id_keys = identity::Keypair::generate_ed25519();
        Ok(id_keys)
    }
    else {
        // TODO: generate keypair from given seed
        let id_keys = identity::Keypair::generate_ed25519();
        Ok(id_keys)
    }
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

    Ok(LemuriaBehaviour { ping, gossipsub, identify, kademlia})
}

