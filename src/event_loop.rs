use std::collections::HashSet;
use futures::StreamExt;
use libp2p::{gossipsub, identify, kad, ping, Multiaddr, PeerId, Swarm};
use libp2p::kad::BootstrapOk;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use tokio::sync::mpsc::Receiver;
use crate::dag::{DagLedger, TransactionWithId, TxHash};
use crate::network::{LemuriaBehaviour, LemuriaBehaviourEvent};
use crate::api::ApiEvent;


pub struct EventLoop {
    swarm: Swarm<LemuriaBehaviour>,
    dag: DagLedger,
    api_receiver: Receiver<ApiEvent>,
    discovered_peers: HashSet<PeerId>,
}

impl EventLoop {

    pub fn new(swarm: Swarm<LemuriaBehaviour>, api_receiver: Receiver<ApiEvent> ) -> Self {
        Self {
            swarm,
            api_receiver,
            dag: DagLedger::new(),
            discovered_peers: HashSet::new(),
        }
    }


    pub async fn run(&mut self) {

        // self.dag.sync_dag().await;

        loop {
            tokio::select! {
                swarm_event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(swarm_event).await;
                }
                Some(api_event) = self.api_receiver.recv() => {
                    self.handle_api_event(api_event).await;
                }
                else => {
                    break;
                }
            }

        }
    }

    /*
    HANDLE SWARM EVENT
     */
    async fn handle_swarm_event(&mut self, event: SwarmEvent<LemuriaBehaviourEvent>) {
        let peer_id = self.swarm.local_peer_id().clone();

        match event {
            SwarmEvent::Behaviour(behaviour_event) => {
                match behaviour_event {
                    LemuriaBehaviourEvent::Ping(ping::Event { peer, result, .. }) => {
                        match result {
                            Ok(rtt) => println!("Ping to {}: {} ms", peer, rtt.as_millis()),
                            Err(e) => println!("Ping error with {}: {:?} ms", peer, e),
                        }
                    }
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
                    LemuriaBehaviourEvent::Kademlia(kad_event) => {
                        match kad_event {
                            // Handle incoming Kademlia requests
                            kad::Event::InboundRequest { request } => {
                                println!("Kad REQUEST: :{:?}", request);
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

                            // Handle routing updates (new peers discovered)
                            kad::Event::RoutingUpdated { peer, is_new_peer, addresses, .. } => {
                                println!("Peer {} discovered, new: {}, addresses: {:?}", peer, is_new_peer, addresses);
                            }

                            // Handle successful DHT queries
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
                            kad::Event::UnroutablePeer { peer } => {
                                println!("Unroutable peer detected: {:?}", peer);
                            },

                            // Catch all unhandled events
                            _ => {
                                println!("Unhandled Kademlia event: {:?}", kad_event);
                            }
                        }
                    }
                    LemuriaBehaviourEvent::Identify(identify_event) => {
                        match identify_event {
                            identify::Event::Received { connection_id, peer_id, info } => {
                                println!("Peer {peer_id} supports protocols: {:?}", info.protocols);
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
                            _ => {
                                println!("Unhandled identify event: {:?}", identify_event);
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
            }
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                println!("Connection closed with {}: {:?}", peer_id, cause);
            }
            SwarmEvent::IncomingConnection { local_addr, send_back_addr, connection_id } => {
                println!("Incoming connection: local: {:?}, remote: {:?}, connection_id: {:?}", local_addr, send_back_addr, connection_id);
            }
            SwarmEvent::NewExternalAddrOfPeer { peer_id, address } => {
                println!("Peer {} announced valid external address: {}", peer_id, address);
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

        if let Ok(tx) = serde_json::from_slice::<TransactionWithId>(&message.data) {

            match self.dag.add_tx(tx.clone()) {
                Ok(()) => {
                    println!("Transaction added to dag: {:?}", tx.id);
                    println!("DAG Current State: {:?}", self.dag.state_tree);
                }
                Err(e) => {
                    eprintln!("Error adding transaction: {:?}", e);

                }
            }
        } else {
            println!("  message: {:?}", std::str::from_utf8(&message.data).unwrap());
        }
    }


    /*
    HANDLE API EVENT
     */
    async fn handle_api_event(&mut self, api_event: ApiEvent) {

        match api_event {

            //Transaction Events
            ApiEvent::SendTx { contract, function, args, respond_to } => {
                self.submit_tx(contract, function, args).await;
            }
            ApiEvent::TxStatus {tx_id, respond_to } => {
                let tx_status = "";
                let _ = respond_to.send(tx_status.parse().unwrap());
            }

            // Network Events
            ApiEvent::DialPeer {peer_id, respond_to} => {
                let _ = respond_to.send(true);
            }
            ApiEvent::GetPeers { respond_to } => {
                let _ = respond_to.send(vec!["peer1".to_string(), "peer2".to_string()]);
            }
            ApiEvent::GetKnownAddresses { respond_to} => {
                let _ = respond_to.send(vec![]);

            }
            ApiEvent::SelfPeerId { respond_to} => {
                let peer_id = self.swarm.local_peer_id().to_string();
                let _ = respond_to.send(peer_id);
            }

            // State Events
            ApiEvent::GetAccount {peer_id, respond_to} => {
                let _ = respond_to.send(vec![]);
            }
            ApiEvent::Bootstrap { bootstrap_addr, respond_to } => {
                println!("Attempting bootstrap to {}", bootstrap_addr);

                if !bootstrap_addr.is_empty() {
                    match bootstrap_addr.parse::<Multiaddr>() {
                        Ok(bootstrap_multiaddr) => {
                            let (peer_address, peer_id) = extract_peer_id(&bootstrap_multiaddr);
                            if let Err(e) = self.swarm.dial(bootstrap_multiaddr) {
                                let _ = respond_to.send(Err(format!("Failed to dial: {:?}", e)));
                                return;
                            }
                            self.swarm.behaviour_mut().kademlia.add_address(&peer_id, peer_address);
                            match self.swarm.behaviour_mut().kademlia.bootstrap() {
                                Ok(_) => {
                                    let _ = respond_to.send(Ok(()));
                                }
                                Err(e) => {
                                    let _ = respond_to.send(Err(format!("Kademlia bootstrap failed: {:?}", e)));
                                }
                            }
                        }
                        Err(e) => {
                            let _ = respond_to.send(Err(format!("Invalid multiaddr: {:?}", e)));
                        }
                    }
                } else {
                    let addrs: Vec<Multiaddr> = Swarm::listeners(&self.swarm)
                        .map(|addr| addr.clone().with(Protocol::P2p((*self.swarm.local_peer_id()).into())))
                        .collect();
                    println!("No bootstrap node provided, starting as bootstrap node...");
                    println!("Bootstrap node addresses: {:?}", addrs);
                    let _ = respond_to.send(Ok(()));
                }
            }
            ApiEvent::GetContract {contract_address, respond_to} => {
                println!("Get contract {:?}", contract_address);
            }
            ApiEvent::GetTx {tx_id, respond_to} => {
                println!("Get tx {:?}", tx_id);
            }
            ApiEvent::GetParents {tx_id, respond_to} => {
                println!("Get parents {:?}", tx_id);
            }
            ApiEvent::GetChildren {tx_id, respond_to} => {
                println!("Get children {:?}", tx_id);
            }

            // Sync events
            ApiEvent::GetSyncStatus { respond_to} => {
                println!("Get sync status");
            }
            ApiEvent::DagHeight{ respond_to}  => {
                println!("Dag height");
            }

            // Debug / Admin
            ApiEvent::Metrics { respond_to}=> {
                println!("Metrics");
            }
            ApiEvent::Shutdown => {
                println!("Shutdown");
            }

            _ => println!("Unhandled api event: {:?}", api_event),
        }
    }

    async fn submit_tx(&mut self, contract: TxHash, function: String, args: Vec<Vec<u8>>) {
        println!("Submitting transaction: contract: {:?}, function: {:?}, args: {:?}", contract, function, args);
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