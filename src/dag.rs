use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::hash::{DefaultHasher, Hasher};
use wasmtime::{Engine};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxHash(pub [u8; 32]);

impl TxHash {
    pub fn from_string(s: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(s.as_bytes());
        let result = hasher.finalize();

        let mut arr = [0u8; 32];
        arr.copy_from_slice(&result);
        TxHash(arr)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StateHash (pub [u8; 32]);


#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ZkProof(pub Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Contract {}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    pub timestamp: u64,
    pub peer_id: String,
    pub signature: Option<Vec<u8>>,
    pub tx_version: u8,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: TxHash,
    pub parents: Vec<TxHash>,
    pub prev_state_root: StateHash,
    pub new_state_root: StateHash,
    pub zk_proof: ZkProof,
    pub data: Option<Vec<u8>>,
    pub metadata: Metadata,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateNode {
    Leaf {
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Branch {
        left: [u8; 32], // Hash of left child
        right: [u8; 32], // Hash of right child
    },
    Empty,
}
#[derive(Debug, Clone)]
pub struct DagLedger {
    pub transactions: HashMap<TxHash, Transaction>,
    pub children: HashMap<TxHash, HashSet<TxHash>>,
    pub state_tree: HashMap<TxHash, StateNode>,
    pub engine: Engine,
}

impl DagLedger {

    pub fn new() -> Self {
        let mut ledger = Self {
            transactions: HashMap::new(),
            children: HashMap::new(),
            state_tree: HashMap::new(),
            engine: Engine::default(),
        };

        // initialize genesis tx's

        ledger
    }

    pub fn tx(
        &mut self,
        contract_address: TxHash,
        contract_function: String,
        contract_args: Vec<u8>
    ) -> Result<Vec<StateHash>, Box<dyn Error>> {
        unimplemented!()
    }

    pub fn add_tx(&mut self, tx: Transaction) -> bool{
        unimplemented!()
    }

    pub async fn sync_dag(&mut self) -> Result<DagLedger, Box<dyn Error>> {
        unimplemented!()
    }
}