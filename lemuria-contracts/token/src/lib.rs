use std::collections::HashMap;
use bincode::{decode_from_slice, encode_to_vec};

extern "C" {
    fn read_state(offset_ptr: i32, key_ptr: i32, key_len: i32) -> i32;
    fn write_state(key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32);
    fn write_output(ptr: i32, len: i32) -> i32;
}

fn read_key(key: &[u8]) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 1024];
    let ptr = buf.as_mut_ptr() as i32;
    let len = unsafe { read_state(ptr, key.as_ptr() as i32, key.len() as i32) };
    if len < 0 {
        None
    } else {
        Some(unsafe { Vec::from(std::slice::from_raw_parts(ptr as *const u8, len as usize)) })
    }
}

fn write_key(key: &[u8], val: &[u8]) {
    unsafe {
        write_state(key.as_ptr() as i32, key.len() as i32, val.as_ptr() as i32, val.len() as i32);
    }
}

fn read_u64(key: &[u8]) -> u64 {
    read_key(key)
        .map(|v| u64::from_le_bytes(v[..8].try_into().unwrap()))
        .unwrap_or(0)
}

fn write_u64(key: &[u8], value: u64) {
    write_key(key, &value.to_le_bytes());
}

fn return_error(message: &str) -> i32 {
    unsafe { write_output(message.as_ptr() as i32, message.len() as i32) };
    1
}

#[no_mangle]
pub extern "C" fn mint(ptr: i32, len: i32) -> i32 {
    let args_bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let (args, _): (HashMap<String, Vec<u8>>, _) =
        match decode_from_slice(args_bytes, bincode::config::standard()) {
            Ok(result) => result,
            Err(e) => return return_error(&format!("Failed to decode args: {}", e)),
        };

    let to = match args.get("to") {
        Some(v) => v.clone(),
        None => return return_error("Missing 'to'"),
    };

    let amount_bytes = match args.get("amount") {
        Some(v) => v,
        None => return return_error("Missing 'amount'"),
    };

    if amount_bytes.len() < 8 {
        return return_error("Amount too short");
    }

    let amount = u64::from_le_bytes(amount_bytes[..8].try_into().unwrap());

    let mut key = b"balance:".to_vec();
    key.extend_from_slice(&to);
    let old_balance = read_u64(&key);
    let new_balance = old_balance + amount;
    write_u64(&key, new_balance);

    let total = read_u64(b"total_supply");
    let new_total = total + amount;
    write_u64(b"total_supply", new_total);

    let mut updates = HashMap::new();
    updates.insert(key, new_balance.to_le_bytes().to_vec());
    updates.insert(b"total_supply".to_vec(), new_total.to_le_bytes().to_vec());

    let encoded = encode_to_vec(updates, bincode::config::standard()).unwrap();
    unsafe { write_output(encoded.as_ptr() as i32, encoded.len() as i32) }
}

#[no_mangle]
pub extern "C" fn transfer(ptr: i32, len: i32) -> i32 {
    let args_bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let (args, _): (HashMap<String, Vec<u8>>, _) =
        match decode_from_slice(args_bytes, bincode::config::standard()) {
            Ok(result) => result,
            Err(e) => return return_error(&format!("Failed to decode args: {}", e)),
        };

    let caller_bytes = match args.get("caller") {
        Some(v) => v.clone(),
        None => return return_error("Missing 'caller'"),
    };

    let to = match args.get("to") {
        Some(v) => v.clone(),
        None => return return_error("Missing 'to'"),
    };

    let amount_bytes = match args.get("amount") {
        Some(v) => v,
        None => return return_error("Missing 'amount'"),
    };

    if amount_bytes.len() < 8 {
        return return_error("Amount too short");
    }

    let amount = u64::from_le_bytes(amount_bytes[..8].try_into().unwrap());

    let mut from_key = b"balance:".to_vec();
    from_key.extend_from_slice(&caller_bytes);
    let from_balance = read_u64(&from_key);

    if from_balance < amount {
        return return_error("Insufficient balance");
    }

    let mut to_key = b"balance:".to_vec();
    to_key.extend_from_slice(&to);
    let to_balance = read_u64(&to_key);

    let new_from_balance = from_balance - amount;
    let new_to_balance = to_balance + amount;

    write_u64(&from_key, new_from_balance);
    write_u64(&to_key, new_to_balance);

    let mut updates = HashMap::new();
    updates.insert(from_key, new_from_balance.to_le_bytes().to_vec());
    updates.insert(to_key, new_to_balance.to_le_bytes().to_vec());

    let encoded = encode_to_vec(updates, bincode::config::standard()).unwrap();
    unsafe { write_output(encoded.as_ptr() as i32, encoded.len() as i32) }
}
