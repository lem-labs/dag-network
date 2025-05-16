use crate::api::ApiEvent;
use crate::dag::{Dag, DagEvent, StateNode, Transaction, TransactionWithId, TxHash};
use crate::network::{DagSyncRequest, DagSyncResponse, LemuriaBehaviour, LemuriaBehaviourEvent, LemuriaRequest, LemuriaResponse};
use bincode::error::EncodeError;
use futures::StreamExt;
use libp2p::gossipsub::IdentTopic;
use libp2p::kad::BootstrapOk;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{Event as RequestResponseEvent, Message as RequestResponseMessage, ResponseChannel};
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::SwarmEvent;
use libp2p::{gossipsub, identify, kad, ping, Multiaddr, PeerId, Swarm};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use tokio::sync::mpsc::Receiver;
use tracing_subscriber::fmt::format;

pub struct EventLoop {
    swarm: Swarm<LemuriaBehaviour>,
    dag: Dag,
    api_receiver: Receiver<ApiEvent>,
    dag_receiver: Receiver<DagEvent>,
    discovered_peers: HashSet<PeerId>,
}

impl EventLoop {

    pub fn new(swarm: Swarm<LemuriaBehaviour>, api_receiver: Receiver<ApiEvent> ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        Self {
            swarm,
            api_receiver,
            dag_receiver: rx,
            dag: Dag::new(tx),
            discovered_peers: HashSet::new(),
        }
    }


    pub async fn run(&mut self) {

        loop {
            tokio::select! {
                swarm_event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(swarm_event).await;
                }
                Some(api_event) = self.api_receiver.recv() => {
                    self.handle_api_event(api_event).await;
                }
                Some(dag_event) = self.dag_receiver.recv() => {
                    self.handle_dag_event(dag_event).await;
                }
            }

        }
    }

    /*
    HANDLE SWARM EVENT
     */
    async fn handle_swarm_event(&mut self, event: SwarmEvent<LemuriaBehaviourEvent>) {
        let _peer_id = self.swarm.local_peer_id().clone();

        match event {
            SwarmEvent::Behaviour(behaviour_event) => {
                match behaviour_event {
                    // --- PING ---
                    LemuriaBehaviourEvent::Ping(ping::Event { peer, result, .. }) => {
                        match result {
                            Ok(rtt) => {} //println!("Ping to {}: {} ms", peer, rtt.as_millis()),
                            Err(e) => println!("Ping error with {}: {:?} ms", peer, e),
                        }
                    }
                    // --- GOSSIPSUB ---
                    LemuriaBehaviourEvent::Gossipsub(gs_event) => {
                        match gs_event {
                            gossipsub::Event::Message { propagation_source, message_id, message } => {
                                self.handle_gossipsub_message(propagation_source, message_id, message).await;
                            }
                            gossipsub::Event::Subscribed { peer_id, topic } => {
                                println!("Peer {} subscribed to topic {}", peer_id, topic);
                            }
                            gossipsub::Event::Unsubscribed { peer_id, topic } => {
                                println!("Peer {} unsubscribed from topic {}", peer_id, topic);
                            }
                            _ => {
                                println!("Unhandled gossipsub event: {:?}", gs_event);
                            }
                        }
                    }
                    // --- KADEMLIA ---
                    LemuriaBehaviourEvent::Kademlia(kad_event) => {
                        match kad_event {
                            // --- INBOUND REQUEST
                            kad::Event::InboundRequest { request } => {
                                match request {
                                    kad::InboundRequest::FindNode { num_closer_peers } => {
                                        println!("Received FindNode request. Closer peers: {}", num_closer_peers);
                                    }
                                    kad::InboundRequest::GetProvider { num_closer_peers, num_provider_peers, .. } => {
                                        println!(
                                            "Received GetProvider request. Closer peers: {}, Provider peers: {}",
                                            num_closer_peers, num_provider_peers
                                        );
                                    }
                                    kad::InboundRequest::PutRecord { record, source, connection } => {
                                        println!("Received PutRecord request");
                                        println!("RECORD: {:?}", record);
                                        println!("SOURCE: {:?}", source);
                                        println!("CONNECTION: {:?}", connection);
                                        if let Some(record) = record {
                                            println!("Received PutRecord request for key: {:?}", record.key);
                                        } else {
                                            println!("PutRecord request received, but no record found.");
                                        }
                                    }
                                    kad::InboundRequest::GetRecord { num_closer_peers, present_locally, .. } => {
                                        println!(
                                            "Received GetRecord request. Closer peers: {}, Present locally: {}",
                                            num_closer_peers, present_locally
                                        );
                                    }
                                    kad::InboundRequest::AddProvider { record, .. } => {
                                        if let Some(record) = record {
                                            println!(
                                                "Received AddProvider request. Key: {:?}, Provider: {:?}",
                                                record.key, record.provider
                                            );
                                        } else {
                                            println!(" AddProvider request received, but no record found.");
                                        }
                                    }

                                }
                            }
                            kad::Event::RoutingUpdated { peer, is_new_peer, addresses, .. } => {
                                println!("Peer {} discovered, new: {}, addresses: {:?}", peer, is_new_peer, addresses);
                                self.discovered_peers.insert(peer);
                            }
                            // --- OUTBOUND QUERY PROGRESSED ---
                            kad::Event::OutboundQueryProgressed { id, ref result, .. } => {
                                println!("Kademlia query progressed: Query ID: {:?}", id.to_string());

                                match result {
                                    kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { key, providers, .. })) => {
                                        for peer in providers {
                                            println!(
                                                "Peer {peer:?} provides key {:?}",
                                                std::str::from_utf8(key.as_ref()).unwrap()
                                            );
                                        }
                                    }
                                    kad::QueryResult::GetProviders(Err(err)) => {
                                        eprintln!("Failed to get providers: {err:?}");
                                    }
                                    kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(
                                                                       kad::PeerRecord {
                                                                           record: kad::Record {
                                                                               key, value, ..
                                                                           },..
                                                                       })
                                                                )) => {
                                        println!(
                                            "Got record {:?} {:?}",
                                            std::str::from_utf8(key.as_ref()).unwrap(),
                                            std::str::from_utf8(&value).unwrap(),
                                        );
                                    }
                                    kad::QueryResult::GetRecord(Ok(_)) => {
                                        println!("Got record Ok(_)");
                                    }
                                    kad::QueryResult::GetRecord(Err(err)) => {
                                        eprintln!("Failed to get record: {err:?}");
                                    }
                                    kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
                                        println!(
                                            "Successfully put record {:?}",
                                            std::str::from_utf8(key.as_ref()).unwrap()
                                        );
                                    }
                                    kad::QueryResult::PutRecord(Err(err)) => {
                                        eprintln!("Failed to put record: {err:?}");
                                    }
                                    kad::QueryResult::StartProviding(Ok(kad::AddProviderOk { key })) => {
                                        println!(
                                            "Successfully put provider record {:?}",
                                            std::str::from_utf8(key.as_ref()).unwrap()
                                        );
                                    }
                                    kad::QueryResult::StartProviding(Err(err)) => {
                                        eprintln!("Failed to put provider record: {err:?}");
                                    }
                                    kad::QueryResult::Bootstrap(Ok(BootstrapOk { peer, num_remaining})) => {
                                        self.discovered_peers.insert(*peer);
                                        println!("Discovered peer during bootstrap: {:?} ({} remaining)",peer, num_remaining);

                                        if *num_remaining == 0 {
                                            println!("Bootstrap complete. Total discovered peers: {}",self.discovered_peers.len());
                                        }
                                    }
                                    kad::QueryResult::Bootstrap(Err(e)) => {
                                        eprintln!("Bootstrap failed: {:?}", e);
                                    }
                                    _ => {}
                                }
                            }
                            // Handle unroutable peers
                            // TODO: what does this mean? It appears in bootstrap node logs, but then peer connects
                            kad::Event::UnroutablePeer { peer } => {
                                println!("Unroutable peer detected: {:?}", peer);
                            },
                            _ => {
                                println!("Unhandled Kademlia event: {:?}", kad_event);
                            }
                        }
                    }
                    // --- IDENTIFY ---
                    LemuriaBehaviourEvent::Identify(identify_event) => {
                        match identify_event {
                            identify::Event::Received { connection_id: _connection_id, peer_id, info } => {
                                println!("Peer {peer_id} supports protocols: {:?}", info.protocols);
                                self.discovered_peers.insert(peer_id);
                            }
                            identify::Event::Sent { connection_id, peer_id } => {
                                println!("Identify sent to {peer_id} on connection {connection_id:?}");
                            }
                            identify::Event::Pushed { peer_id, connection_id, info } => {
                                println!("Identify info pushed to {peer_id} on connection {connection_id:?}. Agent: {}, protocols: {:?}", info.agent_version, info.protocols);
                            }
                            identify::Event::Error { peer_id, connection_id, error } => {
                                eprintln!("Identify error with {peer_id} on connection {connection_id:?}: {error:?}");
                            }
                        }
                    }
                    // --- REQUEST RESPONSE ---
                    LemuriaBehaviourEvent::Rr(event) => {
                        match event {
                            RequestResponseEvent::Message { peer, message, .. } => match message {
                                RequestResponseMessage::Request { request, channel, .. } => {
                                    // --- REQUEST
                                    match request {
                                        LemuriaRequest::DagSync(req) => self.handle_dag_sync_request(peer, channel)
                                        // Handle other LemuriaRequest variants here
                                    }
                                }
                                RequestResponseMessage::Response { response, .. } => {
                                    // --- RESPONSE ---
                                    match response {
                                        LemuriaResponse::DagSync(resp) => self.handle_dag_sync_response(resp, peer)
                                        // Handle other LemuriaResponse variants here
                                    }
                                }
                            },
                            RequestResponseEvent::OutboundFailure { peer, error, request_id, connection_id } => {
                                println!("Outbound request to {:?} (id: {:?}) failed: {:?}", peer, request_id, error);
                            }
                            RequestResponseEvent::InboundFailure { peer, error, request_id, connection_id } => {
                                println!("Inbound response from {:?} (id: {:?}) failed: {:?}", peer, request_id, error);
                            }
                            RequestResponseEvent::ResponseSent { peer, request_id, connection_id } => {
                                println!("Response sent to {:?} (id: {:?})", peer, request_id);
                            }
                        }
                    }
                }
            }
            SwarmEvent::Dialing { peer_id: Some(peer_id), .. } => {
                println!("Dialing peer {}", peer_id);
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                let local_peer_id = *self.swarm.local_peer_id();
                println!("Now listening on {:?}", address.with(Protocol::P2p(local_peer_id)));
            }
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, num_established, .. } => {
                println!("Connection established with {peer_id} via {endpoint:?} ({num_established} total)");
                self.discovered_peers.insert(peer_id);
            }
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                println!("Connection closed with {}: {:?}", peer_id, cause);
                self.discovered_peers.remove(&peer_id);
            }
            SwarmEvent::IncomingConnection { local_addr, send_back_addr, connection_id } => {
                println!("Incoming connection: local: {:?}, remote: {:?}, connection_id: {:?}", local_addr, send_back_addr, connection_id);
            }
            SwarmEvent::NewExternalAddrOfPeer { peer_id, address } => {
                println!("Peer {} announced valid external address: {}", peer_id, address);
                self.discovered_peers.insert(peer_id);
            }
            SwarmEvent::NewExternalAddrCandidate { address } => {
                println!("New external address candidate: {:?}", address)
            }
            SwarmEvent::IncomingConnectionError { local_addr, send_back_addr, connection_id, error } => {
                eprintln!("Incoming connection error: {:?}, {:?}, {:?}, {:?}", local_addr, send_back_addr, connection_id, error);
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                if let Some(peer_id) = peer_id {
                    eprintln!("Failed to connect to {}: {:?}", peer_id, error);
                }
            }
            // Handle unexpected events
            _ => {
                println!("Unhandled swarm event: {:?}", event);
            }
        }

        }

    async fn handle_gossipsub_message(&mut self, propagation_source: PeerId, message_id: gossipsub::MessageId, message: gossipsub::Message) {
        println!("Received message");
        println!("  from: {}", propagation_source);
        println!("  message_id: {}", message_id);

        let config = bincode::config::standard();
        let result: Result<(TransactionWithId, usize), _> = bincode::decode_from_slice(&message.data, config);

        match result {
            Ok((tx_with_id, _bytes_read)) => {
                let tx = Transaction {
                    parents: tx_with_id.parents,
                    contract_call: tx_with_id.contract_call,
                    prev_state_root: tx_with_id.prev_state_root,
                    new_state_root: tx_with_id.new_state_root,
                    data: tx_with_id.data,
                    metadata: tx_with_id.metadata,
                };
                match self.dag.add_tx(tx_with_id.id, tx.clone()) {
                    Ok(()) => {
                        println!("Transaction added to dag: {:?}", tx_with_id.id);
                    }
                    Err(e) => eprintln!("Error adding transaction: {:?}", e)
                }
            }
            Err(e) => eprintln!("Failed to deserialize incoming message: {:?}", e)
        }
    }


    /*
    HANDLE API EVENT
     */
    async fn handle_api_event(&mut self, api_event: ApiEvent) {

        match api_event {
            ApiEvent::Bootstrap { bootstrap_addr, respond_to } => {
                if let Err(e) = self.bootstrap(bootstrap_addr).await {
                    let _ = respond_to.send(Err(e));
                } else {
                    let _ = respond_to.send(Ok(()));
                }
            }
            ApiEvent::InitDag { respond_to } => {
                self.dag.initialize_dag();
                let _ = respond_to.send(Ok(()));
            }
            ApiEvent::SendTx { contract, function, args, respond_to } => {
                println!("Sending Transaction: contract: {:?}, function: {}, args: {:?}", contract, function, args);
                match TxHash::from_hex(&contract) {
                    Ok(tx_hash) => {
                        match self.dag.tx(tx_hash, function, args, self.swarm.local_peer_id().to_string(), 0).await {
                            Ok(tx_hash) => {
                                println!("Transaction sent successfully with id {:?}", tx_hash);
                                let _ = respond_to.send(Ok(tx_hash));
                            }
                            Err(e) => {
                                let error_message = format!("Transaction failed: {:?}", e);
                                eprintln!("{}", error_message);
                                let _ = respond_to.send(Err(error_message));
                            }
                        }
                    } Err(e) => {

                    }
                }

            }
            ApiEvent::SyncDag {peer, respond_to} => {
                let peer_id = PeerId::from_str(&peer).unwrap();
                let request = LemuriaRequest::DagSync(DagSyncRequest {});
                let request_id = self.swarm.behaviour_mut().rr.send_request(&peer_id, request);
                println!("Successfully sent sync dag request with request id: {:?}", request_id);

                let _ = respond_to.send(Ok(()));
            }
            ApiEvent::GetTx {tx_id, respond_to} => {
                println!("Received GetTx request for TxId: {:?}", tx_id);
                match TxHash::from_hex(&tx_id) {
                    Ok(txid) => {
                        if let Some(txc) = self.dag.transactions.get(&txid) {
                            let tx_clone = txc.clone();
                            let _ = respond_to.send(Ok(TransactionWithId {
                                id: txid,
                                parents: tx_clone.parents,
                                contract_call: tx_clone.contract_call,
                                prev_state_root: tx_clone.prev_state_root,
                                new_state_root: tx_clone.new_state_root,
                                data: tx_clone.data,
                                metadata: tx_clone.metadata,
                            }));
                        } else {
                            let _ = respond_to.send(Err(format!("No transaction found with id {:?}", tx_id)));
                        }
                    }
                    Err(e) => {
                        let _ = respond_to.send(Err(format!("Invalid transaction id: {:?}", tx_id)));
                    }
                }
            }
            ApiEvent::GetTxs {tx_ids, respond_to} => {
                let mut txids: Vec<TxHash> = vec![];
                for txid in tx_ids {
                    if let Ok(t) = TxHash::from_hex(&*txid) {
                        txids.push(t);
                    }
                }

                let mut txs: Vec<TransactionWithId> = vec![];
                for txid in txids {
                    if let Some(tx) = self.dag.transactions.get(&txid) {
                        let tx_clone = tx.clone();
                        txs.push(TransactionWithId {
                            id: txid,
                            parents: tx_clone.parents,
                            contract_call: tx_clone.contract_call,
                            prev_state_root: tx_clone.prev_state_root,
                            new_state_root: tx_clone.new_state_root,
                            data: tx_clone.data,
                            metadata: tx_clone.metadata,
                        });
                    }
                }
                let _ = respond_to.send(txs);
            }
            ApiEvent::GetAncestry {tx_id, levels, respond_to} => {
                println!("Received GetAncestry request for TxId: {:?}", tx_id);
                match TxHash::from_hex(&tx_id) {
                    Ok(txid) => {
                        let ancestry = self.dag.get_ancestry(&txid, levels);
                        let _ = respond_to.send(Ok(ancestry));
                    }
                    Err(e) => {
                        let _ = respond_to.send(Err(format!("Error, invalid tx id: {:?}", tx_id)));
                    }
                }

            }
            ApiEvent::GetPeers { respond_to } => {
                println!("Received GetPeers request for TxId");
                let _ = respond_to.send(vec![]);
            }
            ApiEvent::GetKnownAddresses { respond_to} => {
                println!("Received GetKnownAddresses request for TxId");
                let _ = respond_to.send(vec![]);

            }
            ApiEvent::SelfPeerId { respond_to} => {
                println!("Received SelfPeerId request");
                let peer_id = self.swarm.local_peer_id().to_string();
                println!("PeerID: {:?}", peer_id);
                let _ = respond_to.send(peer_id);
            }
            ApiEvent::SelfAddresses {respond_to} => {
                println!("Received SelfAddresses request");
                let addresses = self.swarm.listeners()
                    .map(|addr| addr.to_string())
                    .collect::<Vec<String>>();
                let _ = respond_to.send(addresses);
            }
            ApiEvent::DialPeer {peer_id, multiaddr, respond_to} => {
                println!("Received DialPeer request for PeerId: {:?}", peer_id);
                let dial_opts=  DialOpts::peer_id(peer_id.parse().unwrap()).addresses(vec![multiaddr.parse().unwrap()]).build();
                match self.swarm.dial(dial_opts) {
                    Ok(_) => {let _ = respond_to.send(true);}
                    Err(e) => {let _ = respond_to.send(false);}
                }

            }
            ApiEvent::DagHeight{ respond_to}  => {
                println!("Dag height");
            }

            // Debug / Admin
            ApiEvent::Metrics { respond_to}=> {
                println!("Metrics");
            }
            ApiEvent::Shutdown => {
                println!("Shutdown signal received, exiting...");
                std::process::exit(0);
            }
            _ => println!("Unhandled api event: {:?}", api_event),
        }
    }

    async fn handle_dag_event(&mut self, dag_event: DagEvent) {
        match dag_event {
            DagEvent::Broadcast {txid, tx } => {
                let topic = IdentTopic::new("lemuria-zk-dag");
                let tx_clone = tx.clone();
                let msg = TransactionWithId {
                    id: txid,
                    parents: tx_clone.parents,
                    contract_call: tx_clone.contract_call,
                    prev_state_root: tx_clone.prev_state_root,
                    new_state_root: tx_clone.new_state_root,
                    data: tx_clone.data,
                    metadata: tx_clone.metadata,
                };
                let config = bincode::config::standard();
                let encoded: Result<Vec<u8>, EncodeError> = bincode::encode_to_vec(&msg, config);
                match encoded {
                    Ok(encoded) => {
                        self.swarm.behaviour_mut().gossipsub.publish(topic, encoded).expect("Failed to publish transaction");
                    }
                    Err(e) => {
                        eprintln!("Failed to encode transaction {:?}: {:?}, Error: {:?}", txid, tx, e);
                    }
                }
            }
        }
    }

    async fn bootstrap(&mut self, bootstrap_addr: String) -> Result<(), String> {
        println!("Attempting bootstrap to {}", bootstrap_addr);
        match bootstrap_addr.parse::<Multiaddr>() {
            Ok(bootstrap_multiaddr) => {
                let (peer_address, peer_id) = extract_peer_id(&bootstrap_multiaddr);
                if let Err(e) = self.swarm.dial(bootstrap_multiaddr) {
                    return Err(format!("Bootstrapping failed - Failed to dial: {:?}", e));
                }
                self.swarm.behaviour_mut().kademlia.add_address(&peer_id, peer_address);
                match self.swarm.behaviour_mut().kademlia.bootstrap() {
                    Ok(_) => {
                        println!("Successfully bootstrapped to {}", bootstrap_addr);
                        Ok(())

                    }
                    Err(e) => {
                        Err(format!("Failed to bootstrap peer to {}: {:?}", bootstrap_addr, e))
                    }
                }
            }
            Err(e) => {
                Err(format!("Invalid multiaddr: {:?}", e))
            }
        }
    }

    fn handle_dag_sync_request(&mut self, peer: PeerId, channel: ResponseChannel<LemuriaResponse>) {
        println!("Received DagSync request from {:?}", peer);

        let dag = self.dag.transactions.clone();
        let encoded_dag = bincode::encode_to_vec(&dag, bincode::config::standard()).unwrap_or_default(); // TODO: replace with proper error handling
        let tips_children = self.dag.tips_children.clone();
        let encoded_tips_children = bincode::encode_to_vec(&tips_children, bincode::config::standard()).unwrap_or_default();
        let state_tree = self.dag.state_tree.clone();
        let encoded_state_tree = bincode::encode_to_vec(&state_tree, bincode::config::standard()).unwrap_or_default();
        let response = LemuriaResponse::DagSync(DagSyncResponse { dag_data: encoded_dag, tips_children_data: encoded_tips_children, state_tree_data: encoded_state_tree });

        if let Err(e) = self.swarm.behaviour_mut().rr.send_response(channel, response) {
            eprintln!("Failed to send response to {:?}: {:?}", peer, e);
        }
    }

    fn handle_dag_sync_response(&mut self, resp: DagSyncResponse, peer: PeerId) {
        println!("Received DagSync response from {:?} with {} bytes", peer, resp.dag_data.len());
        let config = bincode::config::standard();
        match bincode::decode_from_slice::<HashMap<TxHash, Transaction>, _>(&resp.dag_data, config) {
            Ok((received_tx_map, _)) => {
                println!("Decoded {} DAG entries", received_tx_map.len());

                match bincode::decode_from_slice::<HashMap<TxHash, HashSet<TxHash>>, _>(&resp.tips_children_data, config) {
                    Ok((received_t_c_map, _)) => {
                        match bincode::decode_from_slice::<HashMap<TxHash, StateNode>, _>(&resp.state_tree_data, config) {
                            Ok((received_state_tree_map, _)) => {
                                for (txid, tx) in received_tx_map {
                                    // update transactions dag
                                    if !self.dag.transactions.contains_key(&txid) {
                                        let _ = self.dag.transactions.insert(txid, tx);
                                    }
                                }
                                // update tips -> children map
                                for (txid, tx) in received_t_c_map {
                                    if !self.dag.tips_children.contains_key(&txid) {
                                        let _ = self.dag.tips_children.insert(txid, tx);
                                    }
                                }
                                for (txid, state_node) in received_state_tree_map {
                                    if !self.dag.state_tree.contains_key(&txid) {
                                        let _ = self.dag.state_tree.insert(txid, state_node);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to decode received state tree from {:?}: {:?}", peer, e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to decode received tips-children from {:?}: {:?}", peer, e);
                    }
                }
            }
            Err(e) =>  eprintln!("Failed to decode DAG sync response: {:?}", e)
        }
    }

}

fn extract_peer_id(addr: &Multiaddr) -> (Multiaddr, PeerId) {
    let cloned_addr = addr.clone();
    let mut components = cloned_addr.into_iter().collect::<Vec<_>>();

    match components.pop() {
        Some(Protocol::P2p(multihash)) => {
            let peer_id = multihash;
            let peer_addr = Multiaddr::from_iter(components);
            (peer_addr, peer_id)
        }
        other => panic!("Expected Multiaddr to end with /p2p/<peer_id>, got {:?}", other),
    }
}

