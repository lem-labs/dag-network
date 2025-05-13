use crate::zk;
use crate::zk::Zkvm;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::hash::Hasher;

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
pub struct TransactionWithId {
    pub id: TxHash,
    pub parents: Vec<TxHash>,
    pub prev_state_root: StateHash,
    pub new_state_root: StateHash,
    pub zk_proof: ZkProof,
    pub data: Option<Vec<u8>>,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
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
    pub zkvm: zk::Zkvm,
}

impl DagLedger {

    pub fn new() -> Self {
        let mut ledger = Self {
            transactions: HashMap::new(),
            children: HashMap::new(),
            state_tree: HashMap::new(),
            zkvm: Zkvm::new(),
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
        /*
        - fetch contract bytecode
        - get updated tips
        - select 2 parents from tips
            - one tx with the most recent state history we are interested in
            - another from the tips that needs a child, random selection of oldest N tips
            - also fetch their state tree paths, parent tx's, etc
        - update local state tree from parents
        - call zk-circuit
            - args: parent_1_tx, parent_2_tx, contract_hash, contract_bytecode, contract_function, contract_args, peer_id, private_key
            - returns: tx_hash, tx_object, state_tree_path, zk_proof
        - update local state tree with returned state tree path
        - add zk_proof to returned tx object
        - call add_tx
        - broadcast tx to network
         */
        unimplemented!()
    }

    pub fn add_tx(&mut self, tx_with_id: TransactionWithId) -> Result<(), Box<dyn Error>> {
        if self.transactions.contains_key(&tx_with_id.id) {
            return Err("Transaction with ID already exists".into());
        }
        /*
       TODO:
       checks before adding to local dag:
       - tx_id = hash(tx.contents)
       - tx.parents are in the dag (and valid tips)
       - tx.timestamp <= now + drift
       - tx.state_root != 0
       - tx.zk_proof != 0
        */

        // Register the transaction
        for parent in &tx_with_id.parents {
            self.children.entry(*parent).or_default().insert(tx_with_id.id);
        }
        let tx = Transaction {
            parents: tx_with_id.parents,
            prev_state_root: tx_with_id.prev_state_root,
            new_state_root: tx_with_id.new_state_root,
            zk_proof: tx_with_id.zk_proof,
            data: tx_with_id.data,
            metadata: tx_with_id.metadata
        };

        self.transactions.insert(tx_with_id.id, tx);

        Ok(())


    }

    pub async fn sync_dag(&mut self) -> Result<DagLedger, Box<dyn Error>> {
        unimplemented!()
    }
}