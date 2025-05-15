# zk-dag
### Zero Knowledge Directed Acyclic Graph
A DAG in which parents are verified and contracts are executed inside a zero knowledge circuit. 

Traditional DAG architectures such as Iota and Avalanche rely on verifier nodes to re-execute their transactions to verify that
they led to the given state change. In our model, instead verifier nodes will only need to verify a zero knowledge proof. This proof
will verify that the transaction correctly verified its parents and that its contract execution led to the given state change. 

This allows you to verify the entire ancestry of a transaction via one single proof, via zk recursion - if you proved your parents 
are valid, that means their parents are valid, which means their parents are valid, and so on.

This also allows for the verifying nodes to not have to know what contract function was called. Since all that is needed to verify
execution is the zk proof, we can avoid including the name of the contract, function, and args in the transaction. State updates will 
still be publicly available, though.

### To Run:
- compile lemutia-contracts:
  - `cd lemuroa-contracts/;`
  - `rustup target add wasm32-unknown-unknown;`
  - `cargo build --release --target wasm32-unknown-unknown` 

- `cargo run` multiple instances
- enter some key seed and ports for each instance
- run `call_api.sh`
  - this will be our nodes' controller
- enter api port of `node_0` (our bootstrap node)
- run `init-dag` command to initialize genesis blocks and initial contracts on bootstrap node
- for each other node:
  - run `port` command to switch to the api port of `node_i`
  - run `bootstrap` command, providing the bootstrap node's multiaddr
    - can be copied from the log `Now listening on /ip4/192.168.1.11/tcp/1110/p2p/12D3KooWL4u77srLWoVh5VWLrh3a9GfUgTGES58uHNYxcVCnF2h6`
    - example: `/ip4/192.168.1.11/tcp/1110/p2p/12D3KooWL4u77srLWoVh5VWLrh3a9GfUgTGES58uHNYxcVCnF2h6`
  - run `sync-dag` to sync with bootstrap node
    - copy last portion of previous multiaddr for peer id 
    - example: `12D3KooWL4u77srLWoVh5VWLrh3a9GfUgTGES58uHNYxcVCnF2h6`

- once a node is bootstrapped and synced, you can run send-tx, get-tx, etc. (run help for more info)
- to send tx, provide contract address, function, and args
  - to upload contract, call `registry` contract (logged by boot node)

Project meant for demo purposes only.
