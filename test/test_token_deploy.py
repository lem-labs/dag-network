#!/usr/bin/env python3
import subprocess
import requests
import time
import base64
import json
import os
from pathlib import Path
from threading import Thread
from queue import Queue, Empty
import sys
import traceback
import shlex
import readline
import base58

PROJECT_ROOT = Path(__file__).resolve().parent.parent
WASM_PATH = PROJECT_ROOT / "lemuria-contracts/target/wasm32-unknown-unknown/release/token.wasm"
APP_BINARY = PROJECT_ROOT / "target/release/dag-network"
START_PORT = 3100
API_BASE_PORT = 9000
(PROJECT_ROOT / "test").mkdir(parents=True, exist_ok=True)

def enqueue_output(pipe, q, logfile):
    for line in iter(pipe.readline, b''):
        decoded = line.decode()
        q.put(decoded)
        logfile.write(decoded)
        logfile.flush()
    pipe.close()

def start_node(index, seed, dag_port, api_port):
    args = [str(APP_BINARY)]
    log_path = PROJECT_ROOT / f"test/node_{index}.log"
    log_file = open(log_path, "w")

    input_text = f"{seed}\n{dag_port}\n{api_port}\n"

    proc = subprocess.Popen(
        args,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        bufsize=1
    )

    q = Queue()
    t = Thread(target=enqueue_output, args=(proc.stdout, q, log_file))
    t.daemon = True
    t.start()

    proc.stdin.write(input_text.encode())
    proc.stdin.flush()
    proc.stdin.close()

    return proc, q

def wait_api_ready(api_port):
    url = f"http://localhost:{api_port}/self-peer-id"
    for attempt in range(30):
        try:
            r = requests.get(url)
            if r.status_code == 200:
                return r.text.strip().replace('"', '')
        except Exception as e:
            print(f"[wait_api_ready:{api_port}] attempt {attempt+1}/30 failed: {e}")
        time.sleep(1)
    raise RuntimeError(f"API not ready on port {api_port}")

def call(api_port, path, method="post", json_data=None):
    url = f"http://localhost:{api_port}/{path}"
    func = getattr(requests, method)
    try:
        r = func(url, json=json_data)
        r.raise_for_status()
        return r.json()
    except requests.RequestException as e:
        print(f"[call:{api_port}/{path}] Request failed: {e}")
        print(f"Response text: {r.text if 'r' in locals() else 'No response'}")
        raise

def wait_for_registry_address(queue):
    print("Waiting for registry contract log line...")
    for _ in range(60):
        try:
            line = queue.get(timeout=1)
            if "Contract registry-upload address:" in line:
                return line.split("TxHash(")[-1].split(")")[0].strip()
        except Empty:
            pass
    raise RuntimeError("Registry contract address not found in logs")

def interactive_shell(api_ports, peer_ids, registry_address, bootstrap_multiaddr):
    print("\n\U0001f4e1 Entering interactive shell. Type 'help' for commands.\n")
    while True:
        try:
            command = input("dag> ").strip()
            if not command:
                continue
            if command in ("exit", "quit"):
                break
            if command == "help":
                print("""\
Commands:
  list                              - Show peer IDs, ports, and contracts
  send <node> <GET|POST> <path>     - Send request (no JSON body)
  send <node> <POST> <path> <json>  - Send request with JSON payload
  exit / quit                       - Exit and shut down all nodes
""")
                continue
            if command == "info":
                print("\n--- Network Info ---")
                print(f"Bootstrap multiaddr: {bootstrap_multiaddr}")
                print(f"Registry contract address: {registry_address}")
                for i, port in enumerate(api_ports):
                    print(f"  Node {i}: API port {port}, Peer ID {peer_ids[i]}")
                print("")
                continue
            if command.startswith("tx"):
                parts = shlex.split(command)
                if len(parts) < 5:
                    print("Usage: tx <node_index> <function> <to_peer_index> <amount>")
                    continue

                try:
                    node = int(parts[1])
                    function = parts[2]
                    to_index = int(parts[3])
                    amount = int(parts[4])
                except ValueError:
                    print("Invalid input. Make sure indices and amount are integers.")
                    continue

                if function not in ("transfer", "mint"):
                    print("Supported functions: transfer, mint")
                    continue

                try:
                    peer_bytes = list(base58.b58decode(peer_ids[to_index]))
                except Exception as e:
                    print(f"Invalid peer ID at index {to_index}: {e}")
                    continue

                tx_body = {
                    "contract": registry_address,
                    "function": function,
                    "args": {
                        "to": peer_bytes,
                        "amount": amount
                    },
                    "data": ""
                }

                url = f"http://localhost:{api_ports[node]}/send-tx"
                print(f"➡️  POST {url}")
                try:
                    r = requests.post(url, json=tx_body)
                    print(f"⬅️  {r.status_code}")
                    print(json.dumps(r.json(), indent=2) if r.ok else r.text)
                except Exception as e:
                    print(f"⚠️  Error during tx call: {e}")

            print("Unknown command. Type 'help'.")
        except KeyboardInterrupt:
            print("\nExiting...")
            break
        except Exception as e:
            print(f"\u26a0\ufe0f  Error: {e}")

def main(num_nodes):
    dag_ports = [START_PORT + i for i in range(num_nodes)]
    api_ports = [API_BASE_PORT + i for i in range(num_nodes)]
    seeds = [f"seed-{i}" for i in range(num_nodes)]
    processes = []
    queues = []

    print(f"Launching {num_nodes} node(s)...")

    try:
        for i in range(num_nodes):
            print(f"Launching node {i}...")
            proc, q = start_node(i, seeds[i], dag_ports[i], api_ports[i])
            processes.append(proc)
            queues.append(q)
            print(f"Node {i} launched.")
        time.sleep(1)
        peer_ids = []
        for i, port in enumerate(api_ports):
            print(f"Waiting for node {i} API on port {port}...")
            peer_id = wait_api_ready(port)
            print(f"Node {i} ready: Peer ID {peer_id}")
            peer_ids.append(peer_id)

        print("Initializing DAG on bootstrap node...")
        call(api_ports[0], "init-dag", method="get")
        registry_address = wait_for_registry_address(queues[0])
        print(f"Registry contract address: {registry_address}")

        bootstrap_multiaddr = call(api_ports[0], "self-addresses", method="get")[0] + "/p2p/" + peer_ids[0]
        print("Bootstrap multiaddr: " + bootstrap_multiaddr)

        for i in range(1, num_nodes):
            print(f"Bootstrapping node {i}...")
            call(api_ports[i], "bootstrap", json_data={"multiaddr": bootstrap_multiaddr})
            call(api_ports[i], "sync-dag", json_data={"peer": peer_ids[0]})
            print(f"Node {i} bootstrapped.")

        print("Reading and encoding token.wasm...")
        with open(WASM_PATH, "rb") as f:
            encoded = base64.b64encode(f.read()).decode()

        print("Converting peer id to byes")
        peer_bytes = list(base58.b58decode(peer_ids[0]))

        print("Deploying token contract...")
        tx_result = call(api_ports[0], "send-tx", json_data={
            "contract": registry_address,
            "function": "register",
            "args": {},
            "data": encoded
        })
        print("TOKEN DEPLOY RESULT: ")
        print(tx_result)
        token_address = tx_result.get("Ok")
        if not token_address:
            print("Error deploying token contract")
            print("Deploy token contract response:", tx_result)
            return

        print(f"Token contract address: {token_address}")

        peer_bytes = list(base58.b58decode(peer_ids[1]))
        amount_bytes = list((1000).to_bytes(8, byteorder='big'))
        print("Minting tokens...")
        tx_result = call(api_ports[1], "send-tx", json_data={
            "contract": token_address,
            "function": "mint",
            "args": {"to": peer_bytes, "amount": amount_bytes},
            "data": ""
        })

        res = tx_result.get("Ok")
        if not res:
            print("Error minting tokens")
            print("Response:", tx_result)
            return


        amount_each = 1000 // num_nodes
        amount_bytes = list((amount_each).to_bytes(8, byteorder='big'))
        print(f"Distributing {amount_each} tokens to each node...")
        for i in range(num_nodes):
            if i != 1:
                peer_bytes = list(base58.b58decode(peer_ids[i]))
                tx_result = call(api_ports[1], "send-tx", json_data={
                    "contract": token_address,
                    "function": "transfer",
                    "args": {"to": peer_bytes, "amount": amount_bytes},
                    "data": ""
                })
                res = tx_result.get("Ok")
                if not res:
                    print(f"Error distributing tokens to {peer_ids[i]}")
                    print("Response:", tx_result)
                time.sleep(1)

        print("Simulating token transfers...")
        peer_bytes = list(base58.b58decode(peer_ids[2]))
        amount_bytes = list((amount_each // 5).to_bytes(8, byteorder='big'))
        call(api_ports[2], "send-tx", json_data={
            "contract": token_address,
            "function": "transfer",
            "args": {"to": peer_bytes, "amount": amount_bytes},
            "data": ""
        })

        peer_bytes = list(base58.b58decode(peer_ids[2]))
        amount_bytes = list((amount_each // 4).to_bytes(8, byteorder='big'))
        call(api_ports[3 % num_nodes], "send-tx", json_data={
            "contract": token_address,
            "function": "transfer",
            "args": {"to": peer_bytes, "amount": amount_bytes},
            "data": ""
        })

        print("\n--- Network Ready ---")
        print(f"Bootstrap node: Peer ID = {peer_ids[0]}")
        print(f"Bootstrap multiaddr: {bootstrap_multiaddr}")
        print(f"Registry contract address: {registry_address}")
        print("All nodes:")
        for i in range(num_nodes):
            print(f"  Node {i}: API port {api_ports[i]}, Peer ID {peer_ids[i]}")

        interactive_shell(api_ports, peer_ids, registry_address, bootstrap_multiaddr)

    except Exception as e:
        print("\nException occurred during test execution:")
        traceback.print_exc()

    finally:
        print("\nShutting down nodes...")
        for p in processes:
            p.terminate()

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: python3 test-token-deploy.py <num_nodes>")
        sys.exit(1)
    try:
        num = int(sys.argv[1])
    except ValueError:
        print("Please provide an integer for <num_nodes>.")
        sys.exit(1)
    main(num)