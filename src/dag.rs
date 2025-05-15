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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub struct ContractCall {
    pub address: TxHash,
    pub function: String,
    pub args: HashMap<String, Vec<u8>>,
}

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
    pub contract_call: ContractCall,
    pub data: Vec<u8>,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct Transaction {
    pub parents: Vec<TxHash>,
    pub prev_state_root: StateHash,
    pub new_state_root: StateHash,
    pub contract_call: ContractCall,
    pub data: Vec<u8>,
    pub metadata: Metadata,
}


#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
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
    sender: tokio::sync::mpsc::Sender<DagEvent>
}

impl Dag {

    pub fn new(sender: tokio::sync::mpsc::Sender<DagEvent>) -> Self {
        let mut ledger = Self {
            transactions: HashMap::new(),
            tips_children: HashMap::new(),
            state_tree: HashMap::new(),
            sender,
        };

        ledger

    }

    pub fn hash_tx(&self, tx: &Transaction) -> TxHash {
        let encoded = bincode::encode_to_vec(tx, bincode::config::standard()).unwrap();
        let hash = Sha256::digest(&encoded);
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&hash);
        TxHash(arr)
    }

    pub async fn tx(
        &mut self,
        contract_address: TxHash,
        contract_function: String,
        contract_args: HashMap<String, Vec<u8>>,
        peer_id: String,
        retry: u8,

    ) -> Result<(TxHash), Box<dyn Error + Send + Sync>> {
        if retry < 8 {
            let cloned_transactions = self.transactions.clone();
            let contract_tx_option = cloned_transactions.get(&contract_address);
            if let Some(contract_tx) = contract_tx_option {
                if let Some(contract) = self.deserialize_contract(contract_tx.data.clone()) {
                    if let Some((parent_1_hash, parent_2_hash)) = self.select_parents() {
                        // todo: get parent states, parent tx's, etc needed in vm

                        let parents = vec![parent_1_hash.clone(), parent_2_hash.clone()];
                        match self.execute_tx(
                            parents.clone(), contract_address, contract.clone(), contract_function, contract_args.clone(), peer_id, retry // private_key: String,
                        ) {
                            Ok((tx_hash, mut tx_object, state_tree_path)) => {
                                self.update_state(state_tree_path);
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
                            }
                            Err(e) => {
                                Err(e)
                            }
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

        } else { Err("Tx retry limit reached".into()) }

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

    fn execute_tx(
        &mut self,
        parents: Vec<TxHash>,
        contract_address: TxHash,
        contract: Contract,
        contract_function: String,
        contract_args: HashMap<String, Vec<u8>>,
        peer_id: String,
        retry: u8
    ) -> Result<(TxHash, Transaction, Vec<StateNode>), Box<dyn Error + Send + Sync>> {
        // TODO:
        //   - verify hash(private_key) == peer_id
        //   - hash contract bytecode and verify it matches hash (proof correct contract was run)

        match self.verify_parents(parents.clone()) {
            Ok(()) => {
                let merged_state = self.merge_parent_states(parents.clone());

                if let state_path = self.run_contract(
                    contract.clone(),
                    contract_function.clone(),
                    contract_args.clone(),
                    merged_state.clone(),
                ) {
                    let tx = Transaction {
                        parents,
                        prev_state_root: StateHash([0u8; 32]), // TODO: merged state root
                        new_state_root: self.hash_state_tree(&state_path.clone()),
                        contract_call: ContractCall {
                            address: contract_address.clone(),
                            function: contract_function.clone(),
                            args: contract_args.clone(),
                        },
                        data: vec![], // todo: if contract_args included data to upload, include here
                        metadata: Metadata {
                            timestamp: current_timestamp(),
                            peer_id,
                            signature: None,
                            tx_version: 1,
                            tags: vec![contract_function.clone()],
                        },
                    };

                    let tx_hash = self.hash_tx(&tx);
                    Ok((tx_hash, tx, state_path))
                } else {
                    Err("Contract execution failed".into())
                }
            }
            Err(e) => {
                Err(e)
            }
        }
    }


    fn select_parents(&mut self) -> Option<(TxHash, TxHash)> {
        let tips: Vec<_> = self.tips_children.iter()
            .filter(|(_, children)| children.len() < 2)
            .map(|(txid, _)| txid)
            .cloned()
            .collect();

        let mut best_score = 0;
        let mut best_pair = None;

        let dag_clone = self.transactions.clone();
        for i in 0..tips.len() {
            for j in i+1..tips.len() {
                let tx1 = dag_clone.get(&tips[i])?;
                let tx2 = dag_clone.get(&tips[j])?;

                let score = self.parent_pair_score(tx1, tx2);
                if score > best_score {
                    best_score = score;
                    best_pair = Some((tips[i], tips[j]));
                }
            }
        }

        best_pair
    }

    fn deserialize_contract(&self, contract_data: Vec<u8>) -> Option<Contract>{
        // TODO: Replace with actual contract parsing logic (e.g. parse ABI or WASM header)
        Some(Contract {})    }

    fn parent_pair_score(&mut self, tx1: &Transaction, tx2: &Transaction) -> u64 {
        let time_proximity = {
            let dt = (tx1.metadata.timestamp as i64 - tx2.metadata.timestamp as i64).abs() as u64;
            100 - dt.min(100)
        };

        let mut ancestry1 = self.get_ancestry(&self.hash_tx(tx1), 3);
        let mut ancestry2 = self.get_ancestry(&self.hash_tx(tx2), 3);

        let overlap = ancestry1.intersection(&ancestry2).count() as u64;
        let diversity = 100 - overlap;

        time_proximity + diversity
    }

    fn update_state(&mut self, state_path: Vec<StateNode>) {
        for node in state_path {
            let hash = self.hash_state_node(&node);
            self.state_tree.insert(TxHash(hash), node);
        }
    }

    fn hash_state_node(&self, node: &StateNode) -> [u8; 32] {
        let encoded = bincode::encode_to_vec(node, bincode::config::standard()).unwrap();
        let hash = Sha256::digest(&encoded);
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&hash);
        arr
    }

    fn verify_parents(&mut self, parents: Vec<TxHash>) -> Result<(), Box<dyn Error + Send + Sync>> {
        /*
        TODO:
            - verify parent_1_tx and parent_2_tx
                - prove:
                    - no conflicts exist in parents states
                    - zk proofs are valid
                    - leads to given state change
         */
        for parent in &parents {
            if !self.transactions.contains_key(parent) {
                return Err(format!("Missing parent: {:?}", parent).into());
            }
        }
        Ok(())
    }

    fn merge_parent_states(&mut self, parents: Vec<TxHash>) -> Vec<StateNode> {
        // TODO: Merkle merge
        // Merge leaves from both parents into a new state vector
        let mut merged = vec![];

        for parent in parents {
            if let Some(tx) = self.transactions.get(&parent) {
                let state_hash_bytes = tx.new_state_root.0;
                let key = TxHash(state_hash_bytes);
                if let Some(state_node) = self.state_tree.get(&key) {
                    merged.push(state_node.clone());
                }
            }
        }

        merged
    }

    fn hash_state_tree(&self, nodes: &Vec<StateNode>) -> StateHash {
        let mut hasher = Sha256::new();
        for node in nodes {
            let encoded = bincode::encode_to_vec(node, bincode::config::standard()).unwrap();
            hasher.update(encoded);
        }
        let result = hasher.finalize();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&result);
        StateHash(arr)    }

    fn run_contract(
        &mut self,
        _contract: Contract,
        _contract_function: String,
        _contract_args: HashMap<String, Vec<u8>>,
        _merged_state: Vec<StateNode>,
    ) ->  Vec<StateNode> {
        // TODO: run wasm contract
        let new_node = StateNode::Leaf {
            key: b"output".to_vec(),
            value: b"result".to_vec(),
        };

        let hash = self.hash_state_node(&new_node);
        vec![new_node]
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
            contract_call: ContractCall {
                address: TxHash([0u8; 32]),
                function: "genesis".parse().unwrap(),
                args: HashMap::new(),
            },
            data: vec![],
            metadata: Metadata {
                timestamp: current_timestamp(),
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
                contract_call: ContractCall {
                    address: TxHash([0u8; 32]),
                    function: "layer2".parse().unwrap(),
                    args: HashMap::new(),
                },
                data: vec![],
                metadata: Metadata {
                    timestamp: current_timestamp(),
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
                contract_call: ContractCall {
                    address: TxHash([0u8; 32]),
                    function: "layer3".parse().unwrap(),
                    args: HashMap::new(),
                },
                data: vec![],
                metadata: Metadata {
                    timestamp: current_timestamp(),
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

            let tx = Transaction {
                parents: selected_parents.clone(),
                prev_state_root: StateHash([0u8; 32]),
                new_state_root: StateHash([0u8; 32]),  // TODO: update state tree
                contract_call: ContractCall {
                    address: TxHash([0u8; 32]),
                    function: "l4".parse().unwrap(),
                    args: HashMap::new(),
                },
                data: code,
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
            // TODO: update state tree

            for parent in &selected_parents {
                self.tips_children.get_mut(parent).unwrap().insert(hash);
            }
        }

    }

}

use std::time::{SystemTime, UNIX_EPOCH};
use serde::__private::de::IdentifierDeserializer;

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}