use std::collections::HashMap;
use bincode::decode_from_slice;

extern "C" {

    fn read_state(offset_ptr: i32, key_ptr: i32, key_len: i32) -> i32;
    fn write_state(key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32);
    fn write_output(ptr: i32, len: i32) -> i32;
}

fn read_key(key: &[u8]) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 1024];
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

fn write_key(key: &[u8], val: &[u8]) {
    unsafe {
        write_state(
            key.as_ptr() as i32,
            key.len() as i32,
            val.as_ptr() as i32,
            val.len() as i32,
        );
    }
}

fn read_u64(key: &[u8]) -> u64 {
    read_key(key)
        .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
        .unwrap_or(0)
}

fn write_u64(key: &[u8], value: u64) {
    write_key(key, &value.to_le_bytes());
}

#[no_mangle]
pub extern "C" fn mint(ptr: i32, len: i32) -> i32 {
    let args_bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let (args, _): (HashMap<String, Vec<u8>>, _) = decode_from_slice(args_bytes, bincode::config::standard()).unwrap();

    let to = args.get("to").expect("Missing 'to'").clone();
    let amount = args.get("amount").expect("Missing 'amount'");
    let amount = u64::from_le_bytes(amount[..8].try_into().unwrap());

    let mut key = b"balance:".to_vec();
    key.extend_from_slice(&to);
    let old_balance = read_u64(&key);
    write_u64(&key, old_balance + amount);

    let total = read_u64(b"total_supply");
    write_u64(b"total_supply", total + amount);

    unsafe { write_output(0, 0) }
}

#[no_mangle]
pub extern "C" fn transfer(ptr: i32, len: i32) -> i32 {
    let args_bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let (args, _): (HashMap<String, Vec<u8>>, _) = decode_from_slice(args_bytes, bincode::config::standard()).unwrap();

    // The caller is the only authorized sender
    let caller_bytes = args.get("caller").expect("Missing 'caller'").clone();
    let caller = String::from_utf8(caller_bytes.clone()).unwrap();
    let to_bytes = args.get("to").expect("Missing 'to'").clone();
    let to = String::from_utf8(to_bytes.clone()).unwrap();
    let amount_bytes = args.get("amount").expect("Missing 'amount'");
    let amount = u64::from_le_bytes(amount_bytes[..8].try_into().unwrap());

    let mut from_key = b"balance:".to_vec();
    from_key.extend_from_slice(caller.as_bytes());
    let from_balance = read_u64(&from_key);

    if from_balance < amount {
        panic!("Insufficient balance");
    }

    let mut to_key = b"balance:".to_vec();
    to_key.extend_from_slice(to.as_bytes());
    let to_balance = read_u64(&to_key);

    write_u64(&from_key, from_balance - amount);
    write_u64(&to_key, to_balance + amount);

    unsafe { write_output(0, 0) }
}

