use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use sha2::{Digest, Sha256};
use bincode::{encode_to_vec, decode_from_slice, Encode, Decode };

#[derive(Serialize, Deserialize, Encode, Decode)]
struct Contract {
    bytes: Vec<u8>,
    metadata: ContractMetadata,
}

#[derive(Serialize, Deserialize, Encode, Decode)]
struct ContractMetadata {
    owner: String,
}

extern "C" {
    fn read_state(offset_ptr: i32, key_ptr: i32, key_len: i32) -> i32;
    fn write_state(key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32);
    fn write_output(ptr: i32, len: i32) -> i32;
}

fn read_key(key: &[u8]) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 4096];
    let ptr = buf.as_mut_ptr() as i32;
    let key_ptr = key.as_ptr() as i32;
    let key_len = key.len() as i32;
    let len = unsafe { read_state(ptr, key_ptr, key_len) };
    if len < 0 {
        None
    } else {
        Some(unsafe { Vec::from(std::slice::from_raw_parts(ptr as *const u8, len as usize)) })
    }
}

fn write_key(key: &[u8], value: &[u8]) {
    unsafe {
        write_state(
            key.as_ptr() as i32,
            key.len() as i32,
            value.as_ptr() as i32,
            value.len() as i32,
        );
    }
}

#[no_mangle]
pub extern "C" fn register(ptr: i32, len: i32) -> i32 {
    // Deserialize args (should include "owner" as UTF-8 bytes)
    let input_bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let (args, _): (HashMap<String, Vec<u8>>, _) = decode_from_slice(input_bytes, bincode::config::standard()).unwrap();

    let caller = args.get("caller").expect("Missing caller").clone();
    let data_to_upload = args.get("upload_data").expect("Missing upload data").clone();

    // Compute contract address
    let hash = Sha256::digest(&data_to_upload);
    let address = hash.to_vec();

    // Check for duplicate
    if read_key(&address).is_some() {
        panic!("File already exists at this address");
    }

    // Build contract object
    let contract = Contract {
        bytes: data_to_upload,
        metadata: ContractMetadata {
            owner: String::from_utf8(caller).unwrap(),
        },
    };

    let encoded = encode_to_vec(&contract, bincode::config::standard()).unwrap();
    write_key(&address, &encoded);

    // Prepare output diff
    let mut response: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    response.insert(b"address".to_vec(), hex::encode(&address).as_bytes().to_vec());

    let out = encode_to_vec(&response, bincode::config::standard()).unwrap();
    unsafe { write_output(out.as_ptr() as i32, out.len() as i32) }
}
