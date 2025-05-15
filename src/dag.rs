use crate::zk;
use crate::zk::Zkvm;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::hash::Hasher;
use std::fmt;

pub enum DagEvent {
    Broadcast { txid: TxHash, tx: Transaction }
}

#[derive( Clone, Copy, PartialEq, Eq, Hash, Deserialize, Encode, Decode)]
pub struct TxHash(pub [u8; 32]);

impl Serialize for TxHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        serializer.serialize_str(&hex::encode(self.0))
    }
}

impl TxHash {
    pub fn from_string(s: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(s.as_bytes());
        let result = hasher.finalize();

        let mut arr = [0u8; 32];
        arr.copy_from_slice(&result);
        TxHash(arr)
    }

    pub fn from_hex(s: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            return Err("Invalid TxHash length".into());
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(TxHash(arr))
    }
}

impl fmt::Debug for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TxHash({})", hex::encode(self.0))
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Encode, Decode)]
pub struct StateHash(pub [u8; 32]);

impl Serialize for StateHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        serializer.serialize_str(&hex::encode(self.0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct ZkProof(pub Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Contract {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub struct Metadata {
    pub timestamp: u64,
    pub peer_id: String,
    pub signature: Option<Vec<u8>>,
    pub tx_version: u8,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct TransactionWithId {
    pub id: TxHash,
    pub parents: Vec<TxHash>,
    pub prev_state_root: StateHash,
    pub new_state_root: StateHash,
    pub zk_proof: ZkProof,
    pub data: HashMap<String, Vec<u8>>,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct Transaction {
    pub parents: Vec<TxHash>,
    pub prev_state_root: StateHash,
    pub new_state_root: StateHash,
    pub zk_proof: ZkProof,
    pub data: HashMap<String, Vec<u8>>,
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
pub struct Dag {
    pub transactions: HashMap<TxHash, Transaction>,
    pub tips_children: HashMap<TxHash, HashSet<TxHash>>,
    pub state_tree: HashMap<TxHash, StateNode>,
    pub zkvm: zk::Zkvm,
    sender: tokio::sync::mpsc::Sender<DagEvent>
}

impl Dag {

    pub fn new(sender: tokio::sync::mpsc::Sender<DagEvent>) -> Self {
        let mut ledger = Self {
            transactions: HashMap::new(),
            tips_children: HashMap::new(),
            state_tree: HashMap::new(),
            zkvm: Zkvm::new(),
            sender,
        };

        ledger

    }

    pub fn initialize_dag(&mut self ) {

        println!("Initializing to new dag for bootstrap node");

        if (self.transactions.len() > 0) {
            panic!("DAG already contains transactions, cannot initialize.")
        }
        // --- Layer 1: Genesis ---
        let genesis_hash = TxHash::from_string("genesis");
        let genesis_tx = Transaction {
            parents: vec![],
            prev_state_root: StateHash([0u8; 32]),
            new_state_root: StateHash([0u8; 32]),
            zk_proof: ZkProof(vec![]),
            data: HashMap::new(),
            metadata: Metadata {
                timestamp: 0,
                peer_id: "genesis".into(),
                signature: None,
                tx_version: 1,
                tags: vec!["genesis".into()],
            },
        };
        self.transactions.insert(genesis_hash, genesis_tx);

        // --- Layer 2: 3 nodes from genesis ---
        let mut layer2 = vec![];
        for i in 0..3 {
            let label = format!("l2-{}", i);
            let hash = TxHash::from_string(&label);
            let tx = Transaction {
                parents: vec![genesis_hash],
                prev_state_root: StateHash([0u8; 32]),
                new_state_root: StateHash([0u8; 32]),
                zk_proof: ZkProof(vec![]),
                data: HashMap::new(),
                metadata: Metadata {
                    timestamp: 1,
                    peer_id: "layer2".into(),
                    signature: None,
                    tx_version: 1,
                    tags: vec![label.clone()],
                },
            };
            self.transactions.insert(hash, tx);
            layer2.push(hash);
        }

        // --- Layer 3: 4 nodes from pairs of layer 2 ---
        let mut layer3 = vec![];
        for i in 0..4 {
            let label = format!("l3-{}", i);
            let hash = TxHash::from_string(&label);
            let parent_1 = &layer2[i % layer2.len()];
            let parent_2 = &layer2[(i + 1) % layer2.len()];

            let tx = Transaction {
                parents: vec![*parent_1, *parent_2],
                prev_state_root: StateHash([0u8; 32]),
                new_state_root: StateHash([0u8; 32]),
                zk_proof: ZkProof(vec![]),
                data: HashMap::new(),
                metadata: Metadata {
                    timestamp: 2,
                    peer_id: "layer3".into(),
                    signature: None,
                    tx_version: 1,
                    tags: vec![label.clone()],
                },
            };
            self.transactions.insert(hash, tx);

            // Only add what may be real tips to the tips (above layers will be removed after new() returns)
            self.tips_children.insert(hash, HashSet::new());

            layer3.push(hash);
        }

        // --- Layer 4: Contract uploads from valid Layer 3 parents ---
        let system_contracts = vec![
            ("token-upload", include_bytes!("../lemuria-contracts/target/wasm32-unknown-unknown/release/token.wasm").to_vec(), "token"),
            ("registry-upload", include_bytes!("../lemuria-contracts/target/wasm32-unknown-unknown/release/registry.wasm").to_vec(), "contract_registry"),
        ];

        for (label, code, tag) in system_contracts {
            let mut selected_parents = vec![];

            // Find 2 layer-3 nodes with < 2 children
            for candidate in &layer3 {
                let child_count = self.tips_children.get(candidate).map(|s| s.len()).unwrap_or(0);
                if child_count < 2 {
                    selected_parents.push(*candidate);
                }
                if selected_parents.len() == 2 {
                    break;
                }
            }

            if selected_parents.len() < 2 {
                panic!("Not enough available parents in Layer 3 with < 2 children");
            }

            let hash = TxHash::from_string(label);
            println!("Address of {} contract: {:?}", label, hex::encode(hash.0));
            let mut data = HashMap::new();
            data.insert("contract".into(), code);
            data.insert("label".into(), tag.as_bytes().to_vec());

            let tx = Transaction {
                parents: selected_parents.clone(),
                prev_state_root: StateHash([0u8; 32]),
                new_state_root: StateHash([0u8; 32]),
                zk_proof: ZkProof(vec![]),
                data,
                metadata: Metadata {
                    timestamp: 3,
                    peer_id: "contract-uploader".into(),
                    signature: None,
                    tx_version: 1,
                    tags: vec![label.into()],
                },
            };

            self.transactions.insert(hash, tx);
            self.tips_children.insert(hash, HashSet::new());

            for parent in &selected_parents {
                self.tips_children.get_mut(parent).unwrap().insert(hash);
            }
        }

    }


    pub async fn tx(
        &mut self,
        contract_address: TxHash,
        contract_function: String,
        contract_args: HashMap<String, Vec<u8>>,

    ) -> Result<(TxHash), Box<dyn Error + Send + Sync>> {
        let cloned_transactions = self.transactions.clone();
        let contract_tx_option = cloned_transactions.get(&contract_address);
        if let Some(contract_tx) = contract_tx_option {
            let contract_args = &contract_tx.data;
            if let Some(contract_bytes) = contract_args.get("contract") {
                let parents_option = self.select_parents();
                if let Some((parent_1_hash, parent_2_hash)) = parents_option {
                    // todo: get parent states, parent tx's, etc needed in zkvm
                    let merged_state = self.merge_parent_states(parent_1_hash, parent_2_hash);
                    let parent_1_tx = cloned_transactions.get(&parent_1_hash).unwrap();
                    let parent_2_tx = cloned_transactions.get(&parent_2_hash).unwrap();

                    if let (
                        tx_hash,
                        mut tx_object,
                        state_tree_path,
                        zk_proof
                    ) = self.zkvm.execute_tx_circuit(
                        parent_1_tx.clone(),
                        parent_1_hash,
                        parent_2_tx.clone(),
                        parent_2_hash,
                        contract_address,
                        contract_bytes.clone(),
                        contract_function,
                        contract_args.clone(),
                        // peer_id: String,
                        // private_key: String,
                    ) {
                        self.update_state(state_tree_path);
                        tx_object.zk_proof = zk_proof;
                        match self.add_tx(tx_hash, tx_object.clone()) {
                            Ok(()) => {
                                match self.sender.send(DagEvent::Broadcast { txid: tx_hash, tx: tx_object }).await {
                                    Ok(()) => Ok(tx_hash),
                                    Err(e) => Err(Box::new(e))
                                }
                            }
                            Err(e) => {
                                Err(e)
                            }
                        }

                    } else {
                        Err("ZK execution failed".into())
                    }
                } else {
                    Err("Failed to find parents".into())
                }
            } else {
                Err(format!("Tx {:?} does not contain a contract", contract_address).into())
            }
        } else {
            Err(format!("Contract not found at address {:?}", contract_address).into())
        }

    }

    pub fn add_tx(&mut self, txid: TxHash, tx: Transaction) -> Result<(), Box<dyn Error + Send + Sync>> {
        println!("Adding tx {:?}", txid);
        if self.transactions.contains_key(&txid) {
            return Err("Transaction with ID already exists in dag".into());
        }
        if self.tips_children.contains_key(&txid) {
            return Err("Transaction with ID already exists in tips_children".into());
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

        // remove parents from tips
        for parent in &tx.parents {
            println!("Checking parent: {:?}", parent);
            if let Some(tip_children) = self.tips_children.get(parent) {
                println!("let Some(tip_children) = self.tips_children.get(parent)");
                if tip_children.len() > 1 {
                    println!(" if tip_children.len() > 1 ");
                    // enforce 2 child policy
                    self.tips_children.remove(parent);
                } else {
                    println!("ELSE");
                    let mut tip_children_clone = tip_children.clone();
                    tip_children_clone.insert(txid);
                    self.tips_children.insert(parent.clone(), tip_children_clone);
                }
            } else {
                println!("ELSE!!!");
                // parent not in tips
                return Err(format!("Bad Parent - parent not in tip: {:?}", parent).into());
            }

        }

        self.transactions.insert(txid, tx);
        self.tips_children.insert(txid, HashSet::new());

        Ok(())
    }

    fn select_parents(&mut self) -> Option<(TxHash, TxHash)> {

        /*
        TODO: choose parents based on state and recency (and/or weight?)
         */
        let tips: Vec<TxHash> = self.tips_children.keys().cloned().collect();
        for i in 0..tips.len() {
            for j in i+1..tips.len() {
                let a = self.transactions.get(&tips[i]);
                let b = self.transactions.get(&tips[j]);
                if let (Some(a_tx), Some(b_tx)) = (a, b) {
                    let mut parents = HashMap::new();
                    parents.insert(tips[i], a_tx);
                    parents.insert(tips[j], b_tx);

                    if Self::are_valid_parents(parents.clone()) {
                        return Some((tips[i], tips[j]))
                    }
                }
            }
        }
        None
    }

    fn are_valid_parents(parents: HashMap<TxHash, &Transaction>) -> bool {
        /*
        TODO
        parents:
        - do not conflict
        - have updated state
        - arent too old or stale
        - maximize cumulative weight
         */
        return true
    }

    fn merge_parent_states(&mut self, parent_1_hash: TxHash, parent_2_hash: TxHash) -> HashMap<TxHash, StateNode> {
        // TODO: merge parent states
        self.state_tree.clone()
    }

    fn update_state(&mut self, state_path: Vec<StateNode>) {
        unimplemented!()
    }

    fn broadcast_tx(&mut self, txid: TxHash, tx: Transaction) {
        unimplemented!()
    }

    pub fn get_ancestry(&mut self, start: &TxHash, levels: usize) -> HashSet<TxHash> {
        let mut visited = HashSet::new();
        let mut frontier = vec![start.clone()];

        for _ in 0..levels {
            let mut next_frontier = Vec::new();
            for txid in frontier {
                if let Some(tx) = self.transactions.get(&txid) {
                    for parent in &tx.parents {
                        if visited.insert(parent.clone()) {
                            next_frontier.push(parent.clone());
                        }
                    }
                }
            }
            frontier = next_frontier;
            if frontier.is_empty() {
                break;
            }
        }
        visited
    }

}