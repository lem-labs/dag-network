use std::collections::{HashMap, HashSet};
use crate::dag::{TransactionWithId, TxHash};
use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use warp::{Filter, Rejection, Reply};

#[derive(Deserialize)]
struct SendTxInput {
    contract: String,
    function: String,
    args: HashMap<String, Vec<u8>>,
    data: String,
}

#[derive(Deserialize)]
struct DialPeerInput {
    peer_id: String,
    multiaddr: String,
}

#[derive(Deserialize)]
struct TxIdInput {
    tx_id: String,
}
#[derive(Deserialize)]
struct TxIdsInput {
    tx_ids: Vec<String>,
}

#[derive(Deserialize)]
struct SyncDagInput {
    peer: String,
}

#[derive(Deserialize)]
struct MultiaddrInput {
    multiaddr: String,
}
#[derive(Deserialize)]
struct GetFamilyInput {
    tx_id: String,
    levels: usize,
}



#[derive(Debug)]
pub enum ApiEvent {
    InitDag { respond_to: oneshot::Sender<Result<(), String>> },
    Bootstrap { bootstrap_addr: String, respond_to: oneshot::Sender<Result<(), String>> },
    SendTx { contract: String, function: String, args: HashMap<String, Vec<u8>>, data_str: String, respond_to: oneshot::Sender<Result<TxHash, String>> },
    SyncDag { peer: String, respond_to: oneshot::Sender<Result<(), String>> },
    GetTx { tx_id: String, respond_to: oneshot::Sender<Result<TransactionWithId, String>> },
    GetTxs{ tx_ids: Vec<String>, respond_to: oneshot::Sender<Vec<TransactionWithId>> },
    GetAncestry { tx_id: String, levels: usize,  respond_to: oneshot::Sender<Result<HashSet<TxHash>, String>> },
    GetPeers { respond_to: oneshot::Sender<Vec<String>> },
    GetKnownAddresses { respond_to: oneshot::Sender<Vec<String>> },
    SelfPeerId { respond_to: oneshot::Sender<String> },
    SelfAddresses { respond_to: oneshot::Sender<Vec<String>> },
    DialPeer { peer_id: String, multiaddr: String, respond_to: oneshot::Sender<bool> },
    DagHeight { respond_to: oneshot::Sender<u32> },
    Metrics { respond_to: oneshot::Sender<String> },
    Shutdown,
}


#[derive(Debug)]
pub struct Api {
    sender: mpsc::Sender<ApiEvent>,
}

#[derive(Debug)]
struct ParseError;

impl warp::reject::Reject for ParseError {}

impl Api {
    pub fn new(sender: mpsc::Sender<ApiEvent>) -> Self {
        Self { sender }
    }

    pub async fn run(&self, port: u16) {
        let sender = self.sender.clone();

        let routes =
            Self::post("send-tx", sender.clone(), |input: SendTxInput, respond_to| {
                ApiEvent::SendTx {contract: input.contract, function: input.function, args: input.args, data_str: input.data, respond_to }
            })
            .or(Self::post("bootstrap", sender.clone(), |input: MultiaddrInput, respond_to| {
                ApiEvent::Bootstrap { bootstrap_addr: input.multiaddr, respond_to }
            }))
            .or(Self::get("init-dag", sender.clone(), |respond_to| {
                ApiEvent::InitDag { respond_to }
            }))
            .or(Self::post("sync-dag", sender.clone(), | input: SyncDagInput, respond_to| {
                ApiEvent::SyncDag { peer: input.peer, respond_to }
            }))
            .or(Self::post("get-tx", sender.clone(), |input: TxIdInput, respond_to| {
                ApiEvent::GetTx { tx_id: input.tx_id, respond_to }
            }))
            .or(Self::post("get-txs", sender.clone(), |input: TxIdsInput, respond_to| {
                ApiEvent::GetTxs { tx_ids: input.tx_ids, respond_to }
            }))
            .or(Self::post("get-ancestry", sender.clone(), |input: GetFamilyInput, respond_to| {
                ApiEvent::GetAncestry { tx_id: input.tx_id, levels: input.levels, respond_to }
            }))
            .or(Self::get("get-peers", sender.clone(), |respond_to| {
                ApiEvent::GetPeers { respond_to }
            }))
            .or(Self::get("get-known-addresses", sender.clone(), |respond_to| {
                ApiEvent::GetKnownAddresses { respond_to }
            }))
            .or(Self::get("self-peer-id", sender.clone(), |respond_to| {
                ApiEvent::SelfPeerId { respond_to }
            }))
            .or(Self::get("self-addresses", sender.clone(), | respond_to| {
                ApiEvent::SelfAddresses { respond_to }
            }))
            .or(Self::post("dial-peer", sender.clone(), |input: DialPeerInput, respond_to| {
                ApiEvent::DialPeer { peer_id: input.peer_id, multiaddr: input.multiaddr, respond_to }
            }))
            .or(Self::get("dag-height", sender.clone(), |respond_to| {
                ApiEvent::DagHeight { respond_to }
            }))
            .or(Self::get("metrics", sender.clone(), |respond_to| {
                ApiEvent::Metrics { respond_to }
            }));

        warp::serve(routes).run(([0, 0, 0, 0], port)).await;
    }


    fn post<T, R, F>(
        path: &'static str,
        sender: mpsc::Sender<ApiEvent>,
        build_event: F,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone
    where
        T: serde::de::DeserializeOwned + Send + 'static,
        R: Serialize + Send + 'static,
        F: Fn(T, oneshot::Sender<R>) -> ApiEvent + Clone + Send + 'static,
    {
        warp::path(path)
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |input: T| {
                let sender = sender.clone();
                let build_event = build_event.clone();
                async move {
                    let (tx, rx) = oneshot::channel();
                    sender.send(build_event(input, tx)).await.map_err(|_| warp::reject::reject())?;

                    match rx.await {
                        Ok(value) => Ok::<_, Rejection>(warp::reply::json(&value).into_response()),
                        Err(_) => Err(warp::reject::reject()),
                    }
                }
            })
    }

    fn get<R, F>(
        path: &'static str,
        sender: mpsc::Sender<ApiEvent>,
        build_event: F,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone
    where
        R: Serialize + Send + 'static,
        F: Fn(oneshot::Sender<R>) -> ApiEvent + Clone + Send + 'static,
    {
        warp::path(path)
            .and(warp::get())
            .and_then(move || {
                let sender = sender.clone();
                let build_event = build_event.clone();

                async move {
                    let (tx, rx) = oneshot::channel();
                    sender.send(build_event(tx)).await.map_err(|_| warp::reject::reject())?;

                    match rx.await {
                        Ok(data) => Ok::<_, Rejection>(warp::reply::json(&data).into_response()),
                        Err(_) => Err(warp::reject::reject()),
                    }
                }
            })
    }


}
