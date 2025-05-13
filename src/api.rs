use crate::dag::{TransactionWithId, TxHash};
use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use warp::{Filter, Rejection, Reply};

#[derive(Deserialize)]
struct SendTxInput {
    contract: String,
    function: String,
    args: Vec<Vec<u8>>,
}

#[derive(Deserialize)]
struct DialPeerInput {
    peer_id: String,
}

#[derive(Deserialize)]
struct TxIdInput {
    tx_id: TxHash,
}

#[derive(Deserialize)]
struct PeerIdInput {
    peer_id: String,
}

#[derive(Deserialize)]
struct MultiaddrInput {
    multiaddr: String,
}
#[derive(Debug)]
pub enum ApiEvent {
    SendTx { contract: TxHash, function: String, args: Vec<Vec<u8>>, respond_to: oneshot::Sender<TxHash> },
    TxStatus { tx_id: TxHash, respond_to: oneshot::Sender<String> },
    DialPeer { peer_id: String, respond_to: oneshot::Sender<bool> },
    GetPeers { respond_to: oneshot::Sender<Vec<String>> },
    GetKnownAddresses { respond_to: oneshot::Sender<Vec<Multiaddr>> },
    SelfPeerId { respond_to: oneshot::Sender<String> },
    Bootstrap { bootstrap_addr: String, respond_to: oneshot::Sender<Result<(), String>> },
    GetAccount { peer_id: String, respond_to: oneshot::Sender<Vec<u8>> },
    GetContract { contract_address: TxHash, respond_to: oneshot::Sender<Vec<u8>> },
    GetTx { tx_id: TxHash, respond_to: oneshot::Sender<TransactionWithId> },
    GetParents { tx_id: TxHash, respond_to: oneshot::Sender<Vec<TxHash>> },
    GetChildren { tx_id: TxHash,  respond_to: oneshot::Sender<Vec<TxHash>> },
    GetSyncStatus { respond_to: oneshot::Sender<String> },
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
            Self::post("send_tx", sender.clone(), |input: SendTxInput, respond_to| {
                ApiEvent::SendTx {
                    contract: TxHash::from_string(&input.contract),
                    function: input.function,
                    args: input.args,
                    respond_to,
                }
            })
                .or(Self::post("tx_status", sender.clone(), |input: TxIdInput, respond_to| {
                    ApiEvent::TxStatus { tx_id: input.tx_id, respond_to }
                }))
                .or(Self::post("dial_peer", sender.clone(), |input: DialPeerInput, respond_to| {
                    let peer_id = input.peer_id.parse().expect("Invalid PeerId");
                    ApiEvent::DialPeer { peer_id, respond_to }
                }))
                .or(Self::post("bootstrap", sender.clone(), |input: MultiaddrInput, respond_to| {
                    ApiEvent::Bootstrap {
                        bootstrap_addr: input.multiaddr,
                        respond_to,
                    }
                }))
                .or(Self::post("get_account", sender.clone(), |input: PeerIdInput, respond_to| {
                    let peer_id = input.peer_id;
                    ApiEvent::GetAccount { peer_id, respond_to }
                }))
                .or(Self::post("get_contract", sender.clone(), |input: TxIdInput, respond_to| {
                    ApiEvent::GetContract { contract_address: input.tx_id, respond_to }
                }))
                .or(Self::post("get_tx", sender.clone(), |input: TxIdInput, respond_to| {
                    ApiEvent::GetTx { tx_id: input.tx_id, respond_to }
                }))
                .or(Self::post("get_parents", sender.clone(), |input: TxIdInput, respond_to| {
                    ApiEvent::GetParents { tx_id: input.tx_id, respond_to }
                }))
                .or(Self::post("get_children", sender.clone(), |input: TxIdInput, respond_to| {
                    ApiEvent::GetChildren { tx_id: input.tx_id, respond_to }
                }))
                .or(Self::get("get_peers", sender.clone(), |respond_to| {
                    ApiEvent::GetPeers { respond_to }
                }))
                .or(Self::get("get_known_addresses", sender.clone(), |respond_to| {
                    ApiEvent::GetKnownAddresses { respond_to }
                }))
                .or(Self::get("self_peer_id", sender.clone(), |respond_to| {
                    ApiEvent::SelfPeerId { respond_to }
                }))
                .or(Self::get("get_sync_status", sender.clone(), |respond_to| {
                    ApiEvent::GetSyncStatus { respond_to }
                }))
                .or(Self::get("dag_height", sender.clone(), |respond_to| {
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
