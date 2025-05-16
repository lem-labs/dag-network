use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::hash::Hasher;
use std::fmt;
use wasmtime::{Engine, Store, Module, Instance, Linker, TypedFunc};
use anyhow::Result;
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct ContractMetadata {
    owner: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct Contract {
    pub bytes: Vec<u8>,
    pub metadata: ContractMetadata,

}

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
                if let Ok((contract, _)) = bincode::decode_from_slice::<Contract, _>(&contract_tx.data.clone(), bincode::config::standard()) {
                    if let Some((parent_1_hash, parent_2_hash)) = self.select_parents() {
                        // todo: get parent states, parent tx's, etc needed in vm

                        let parents = vec![parent_1_hash.clone(), parent_2_hash.clone()];
                        match self.execute_tx(
                            parents.clone(), contract_address, contract.clone(), contract_function, contract_args.clone(), peer_id, retry // private_key: String,
                        ) {
                            Ok((tx_hash, mut tx_object, state_tree_path)) => {
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
        let expected_hash = self.hash_tx(&tx);
        if expected_hash != txid {
            return Err("Transaction ID does not match computed hash".into());
        }
        /*
       TODO:
           checks before adding to local dag:
           - tx.parents are in the dag (and valid tips)
           - tx.timestamp <= now + drift
           - tx.state_root != 0
           - tx.zk_proof != 0
        */

        // remove parents from tips
        for parent in &tx.parents {
            if let Some(tip_children) = self.tips_children.get(parent) {
                if tip_children.len() > 1 {
                    self.tips_children.remove(parent);
                } else {
                    let mut tip_children_clone = tip_children.clone();
                    tip_children_clone.insert(txid);
                    self.tips_children.insert(parent.clone(), tip_children_clone);
                }
            } else {
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
        // this function will later be executed in zkvm
        // TODO:
        //   - verify hash(private_key) == peer_id
        //   - hash contract bytecode and verify it matches hash (proof correct contract was run)

        match self.verify_parents(parents.clone()) {
            Ok(merged_state_map) => {
                let (merged_root, merged_path) = self.apply_state_diff(merged_state_map.clone());

                if let (new_state_root, state_path) = self.run_contract(
                    contract.clone(),
                    contract_function.clone(),
                    contract_args.clone(),
                    merged_path.clone(),
                ) {
                    let tx = Transaction {
                        parents,
                        prev_state_root: merged_root, // TODO: merged state root
                        new_state_root,
                        contract_call: ContractCall {
                            address: contract_address.clone(),
                            function: contract_function.clone(),
                            args: contract_args.clone(),
                        },
                        data: vec![], // TODO: if contract_args included data to upload, include here
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

    fn build_merkle_tree(&mut self, leaves: HashMap<Vec<u8>, Vec<u8>>) -> (StateHash, Vec<StateNode>) {
        let mut leaf_nodes: Vec<(Vec<u8>, StateNode)> = leaves
            .into_iter()
            .map(|(key, value)| {
                let node = StateNode::Leaf {
                    key: key.clone(),
                    value,
                };
                (key, node)
            })
            .collect();

        // Sort by key for determinism
        leaf_nodes.sort_by(|a, b| a.0.cmp(&b.0));

        let mut hashes_and_nodes: Vec<([u8; 32], StateNode)> = leaf_nodes
            .into_iter()
            .map(|(_, node)| {
                let hash = self.hash_state_node(&node);
                (hash, node)
            })
            .collect();

        if hashes_and_nodes.is_empty() {
            let empty = StateNode::Empty;
            let hash = self.hash_state_node(&empty);
            self.state_tree.insert(TxHash(hash), empty.clone());
            return (StateHash(hash), vec![empty]);
        }

        let mut state_path = Vec::new();

        while hashes_and_nodes.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in hashes_and_nodes.chunks(2) {
                if chunk.len() == 2 {
                    let (left_hash, left_node) = &chunk[0];
                    let (right_hash, right_node) = &chunk[1];
                    let branch = StateNode::Branch {
                        left: *left_hash,
                        right: *right_hash,
                    };
                    let branch_hash = self.hash_state_node(&branch);
                    state_path.push(left_node.clone());
                    state_path.push(right_node.clone());
                    state_path.push(branch.clone());
                    next_level.push((branch_hash, branch));
                } else {
                    let (only_hash, only_node) = &chunk[0];
                    state_path.push(only_node.clone());
                    next_level.push((*only_hash, only_node.clone()));
                }
            }
            hashes_and_nodes = next_level;
        }

        let (root_hash, root_node) = hashes_and_nodes[0].clone();
        state_path.push(root_node.clone());
        self.state_tree.insert(TxHash(root_hash), root_node);
        (StateHash(root_hash), state_path)
    }

    pub fn apply_state_diff(&mut self, updates: HashMap<Vec<u8>, Vec<u8>>) -> (StateHash, Vec<StateNode>) {
        let (root, path) = self.build_merkle_tree(updates);
        self.update_state(path.clone());
        (root, path)
    }


    fn collect_state_from_root(&self, root: &StateHash) -> Option<HashMap<Vec<u8>, Vec<u8>>> {
        let root_hex = hex::encode(root.0);

        if !self.state_tree.contains_key(&TxHash(root.0)) {
            eprintln!("ERROR: State root {} not found in state tree!", root_hex);
            return None;
        }

        let mut result = HashMap::new();
        let mut stack = vec![TxHash(root.0)];

        while let Some(current) = stack.pop() {
            let current_hex = hex::encode(current.0);
            match self.state_tree.get(&current) {
                Some(node) => match node {
                    StateNode::Leaf { key, value } => {
                        result.insert(key.clone(), value.clone());
                    }
                    StateNode::Branch { left, right } => {
                        stack.push(TxHash(*left));
                        stack.push(TxHash(*right));
                    }
                    StateNode::Empty => {
                        eprintln!("Empty node encountered at {}", current_hex);
                    }
                },
                None => {
                    eprintln!("ERROR: Missing state node with hash {}", current_hex);
                }
            }
        }

        if result.is_empty() {
            eprintln!("Final state for root {} is EMPTY", root_hex);
            None
        } else {
            Some(result)
        }
    }

    fn verify_parents(&mut self, parents: Vec<TxHash>) -> Result<HashMap<Vec<u8>, Vec<u8>>, Box<dyn Error + Send + Sync>> {
        if parents.len() != 2 {
            return Err("Exactly two parents are required".into());
        }

        let tx1 = self.transactions.get(&parents[0])
            .ok_or_else(|| format!("Missing parent transaction: {:?}", parents[0]))?.clone();
        let tx2 = self.transactions.get(&parents[1])
            .ok_or_else(|| format!("Missing parent transaction: {:?}", parents[1]))?.clone();

        let state1 = self.collect_state_from_root(&tx1.new_state_root)
            .ok_or_else(|| format!("Failed to collect state from parent {:?}", parents[0]))?;
        let state2 = self.collect_state_from_root(&tx2.new_state_root)
            .ok_or_else(|| format!("Failed to collect state from parent {:?}", parents[1]))?;

        // Detect conflicts
        for (key, val1) in &state1 {
            if let Some(val2) = state2.get(key) {
                if val1 != val2 {
                    return Err(format!("State conflict: key {:?} differs between parents", key).into());
                }
            }
        }

        // Merge state
        let mut merged_state = state1.clone();
        for (key, val) in state2 {
            merged_state.entry(key).or_insert(val);
        }

        match self.verify_parent_execution(tx1, merged_state.clone()) {
            Ok(()) => {
                match self.verify_parent_execution(tx2, merged_state.clone()) {
                    Ok(()) => Ok(merged_state),
                    Err(e) => Err(e)
                }
            }
            Err(e) => Err(e)
        }
    }

    fn verify_parent_execution(&mut self, tx: Transaction, merged_state: HashMap<Vec<u8>, Vec<u8>>) -> Result<(), Box<dyn Error + Send + Sync>> {
        if tx.contract_call.address.0 == [0u8; 32] {
            return Ok(()); // system tx, skip contract validation
        }
        let contract_tx = self.transactions.get(&tx.contract_call.address)
            .ok_or_else(|| format!("Missing contract upload tx: {:?}", tx.contract_call.address))?;

        let contract: Contract = bincode::decode_from_slice::<Contract, _>(&contract_tx.data, bincode::config::standard())?.0;

        let (new_root, state_path) = self.run_contract(
            contract,
            tx.contract_call.function.clone(),
            tx.contract_call.args.clone(),
            merged_state.iter().map(|(k, v)| {
                StateNode::Leaf { key: k.clone(), value: v.clone() }
            }).collect()
        );
        self.update_state(state_path.clone());

        // Reconstruct state root from the returned path to confirm consistency
        let recomputed_root = self.hash_state_node(state_path.last().expect("state_path should not be empty"));
        if StateHash(recomputed_root) != new_root {
            return Err("State path does not reconstruct expected state root".into());
        }
        Ok(())
    }

    fn merge_parent_states(&mut self, parents: Vec<TxHash>) -> (StateHash, Vec<StateNode>) {
        let mut combined = HashMap::new();

        for parent in parents.iter() {
            if let Some(tx) = self.transactions.get(parent) {
                if let Some(state) = self.collect_state_from_root(&tx.new_state_root) {
                    for (k, v) in state {
                        combined.insert(k, v);
                    }
                }
            }
        }

        self.apply_state_diff(combined)
    }


    fn run_contract(
        &mut self,
        contract: Contract,
        contract_function: String,
        contract_args: HashMap<String, Vec<u8>>,
        merged_state: Vec<StateNode>,
    ) -> (StateHash, Vec<StateNode>) {
        use wasmtime::{Engine, Store, Module, Instance, Linker, TypedFunc, Memory};
        use anyhow::Result;

        let engine = Engine::default();
        let mut store = Store::new(&engine, ());
        let module = match Module::from_binary(&engine, &contract.bytes) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("WASM compile error: {}", e);
                return (StateHash([0u8; 32]), vec![]);
            }
        };

        let linker = Linker::new(&engine);
        let instance = match linker.instantiate(&mut store, &module) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("WASM instantiation error: {}", e);
                return (StateHash([0u8; 32]), vec![]);
            }
        };

        let memory = match instance.get_memory(&mut store, "memory") {
            Some(m) => m,
            None => {
                eprintln!("Contract missing `memory` export");
                return (StateHash([0u8; 32]), vec![]);
            }
        };

        // Get the contract call function
        let func: TypedFunc<(i32, i32), i32> = match instance.get_typed_func(&mut store, &contract_function) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Missing export `{}`: {}", contract_function, e);
                return (StateHash([0u8; 32]), vec![]);
            }
        };

        // Convert args to a serialized map
        let mut inputs = HashMap::new();
        for (k, v) in contract_args {
            inputs.insert(k, v); // keys are already String
        }

        let input_bytes = match bincode::encode_to_vec(&inputs, bincode::config::standard()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Arg serialization failed: {}", e);
                return (StateHash([0u8; 32]), vec![]);
            }
        };

        // Write input to WASM memory at offset 0
        let input_ptr = 0;
        if let Err(e) = memory.write(&mut store, input_ptr, &input_bytes) {
            eprintln!("Failed to write input to memory: {}", e);
            return (StateHash([0u8; 32]), vec![]);
        }

        // Call the WASM function
        let result_ptr = match func.call(&mut store, (input_ptr as i32, input_bytes.len() as i32)) {
            Ok(ptr) => ptr,
            Err(e) => {
                eprintln!("Contract execution failed: {}", e);
                return (StateHash([0u8; 32]), vec![]);
            }
        };

        // Read 4 bytes from result_ptr to get the result length
        let mut len_buf = [0u8; 4];
        if let Err(e) = memory.read(&mut store, result_ptr as usize, &mut len_buf) {
            eprintln!("Failed to read result length: {}", e);
            return (StateHash([0u8; 32]), vec![]);
        }
        let result_len = u32::from_le_bytes(len_buf) as usize;

        // Now read the result payload
        let mut result_buf = vec![0u8; result_len];
        if let Err(e) = memory.read(&mut store, result_ptr as usize + 4, &mut result_buf) {
            eprintln!("Failed to read result data: {}", e);
            return (StateHash([0u8; 32]), vec![]);
        }

        // Deserialize the output state diff
        let updates: HashMap<Vec<u8>, Vec<u8>> = match bincode::decode_from_slice(&result_buf, bincode::config::standard()) {
            Ok((u, _)) => u,
            Err(e) => {
                eprintln!("Result deserialization failed: {}", e);
                return (StateHash([0u8; 32]), vec![]);
            }
        };

        self.apply_state_diff(updates)
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

    pub fn initialize_dag(&mut self) {
        println!("Initializing DAG for bootstrap node");

        if !self.transactions.is_empty() {
            panic!("DAG already contains transactions, cannot initialize.");
        }

        // --- Layer 1: Genesis ---
        let genesis_hash = TxHash::from_string("genesis");
        let (genesis_root, genesis_path) = self.apply_state_diff(HashMap::new());

        let genesis_tx = Transaction {
            parents: vec![],
            prev_state_root: StateHash([0u8; 32]),
            new_state_root: genesis_root,
            contract_call: ContractCall {
                address: TxHash([0u8; 32]),
                function: "genesis".to_string(),
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

        // --- Layer 2: From Genesis ---
        let mut layer2 = vec![];
        for i in 0..3 {
            let label = format!("l2-{}", i);
            let updates = HashMap::from([(label.as_bytes().to_vec(), b"layer2".to_vec())]);
            let (new_state_root, path) = self.apply_state_diff(updates);

            let hash = TxHash::from_string(&label);
            let tx = Transaction {
                parents: vec![genesis_hash],
                prev_state_root: genesis_root,
                new_state_root,
                contract_call: ContractCall {
                    address: TxHash([0u8; 32]),
                    function: "layer2".to_string(),
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

        // --- Layer 3: From Layer 2 ---
        let mut layer3 = vec![];
        for i in 0..4 {
            let label = format!("l3-{}", i);
            let parent_1 = layer2[i % layer2.len()];
            let parent_2 = layer2[(i + 1) % layer2.len()];

            let (merged_root, merged_path) = self.merge_parent_states(vec![parent_1, parent_2]);
            self.update_state(merged_path.clone());

            let updates = HashMap::from([(label.as_bytes().to_vec(), b"layer3".to_vec())]);
            let (new_state_root, path) = self.apply_state_diff(updates);

            let hash = TxHash::from_string(&label);

            let tx = Transaction {
                parents: vec![parent_1, parent_2],
                prev_state_root: merged_root,
                new_state_root,
                contract_call: ContractCall {
                    address: TxHash([0u8; 32]),
                    function: "layer3".to_string(),
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
            self.tips_children.insert(hash, HashSet::new());
            layer3.push(hash);
        }

        // --- Layer 4: Upload Contracts ---
        let system_contracts = vec![
            (
                "registry-upload",
                include_bytes!("../lemuria-contracts/target/wasm32-unknown-unknown/release/registry.wasm").to_vec(),
                "contract_registry"
            ),
        ];

        for (label, code, tag) in system_contracts {
            let mut selected_parents = vec![];

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
                panic!("Not enough Layer 3 nodes with <2 children");
            }

            let contract = Contract {
                bytes: code,
                metadata: ContractMetadata {
                    owner: hex::encode(genesis_hash.0),
                },
            };
            let encoded = bincode::encode_to_vec(contract, bincode::config::standard()).unwrap();

            // Dummy state update for the contract label
            let updates = HashMap::from([(label.as_bytes().to_vec(), tag.as_bytes().to_vec())]);
            let (merged_root, merged_path) = self.merge_parent_states(selected_parents.clone());
            self.update_state(merged_path);
            let (new_state_root, path) = self.apply_state_diff(updates);

            let hash = TxHash::from_string(label);
            println!("Contract {} address: {:?}", label, hash);

            let tx = Transaction {
                parents: selected_parents.clone(),
                prev_state_root: merged_root,
                new_state_root,
                contract_call: ContractCall {
                    address: TxHash([0u8; 32]),
                    function: "init".to_string(),
                    args: HashMap::new(),
                },
                data: encoded,
                metadata: Metadata {
                    timestamp: current_timestamp(),
                    peer_id: "contract-uploader".into(),
                    signature: None,
                    tx_version: 1,
                    tags: vec![tag.into()],
                },
            };

            self.transactions.insert(hash, tx);
            self.tips_children.insert(hash, HashSet::new());

            for parent in &selected_parents {
                self.tips_children.get_mut(parent).unwrap().insert(hash);
            }
        }
    }

    pub fn print_dag(&self) {
        println!("=== DAG Transactions ===");
        for (txid, tx) in &self.transactions {
            println!("Tx ID: {}", hex::encode(txid.0));
            println!("  Parents: {:?}", tx.parents.iter().map(|p| hex::encode(p.0)).collect::<Vec<_>>());
            println!("  Prev State Root: {}", hex::encode(tx.prev_state_root.0));
            println!("  New State Root:  {}", hex::encode(tx.new_state_root.0));
            println!("  Function: {}", tx.contract_call.function);
            println!("  ---");
        }
    }

    pub fn print_state_tree(&self) {
        println!("=== State Tree ===");
        for (hash, node) in &self.state_tree {
            println!("Node Hash: {}", hex::encode(hash.0));
            match node {
                StateNode::Leaf { key, value } => {
                    println!("  Type: Leaf");
                    println!("  Key: {:?}", key);
                    println!("  Value: {:?}", value);
                }
                StateNode::Branch { left, right } => {
                    println!("  Type: Branch");
                    println!("  Left: {}", hex::encode(left));
                    println!("  Right: {}", hex::encode(right));
                }
                StateNode::Empty => {
                    println!("  Type: Empty");
                }
            }
            println!("---");
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