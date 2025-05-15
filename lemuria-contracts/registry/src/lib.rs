#[no_mangle]
pub extern "C" fn deploy() {
    let msg = "Contract deployed";
    unsafe {
        print_log(msg.as_ptr(), msg.len());
    }
}

extern "C" {
    fn print_log(ptr: *const u8, len: usize);
}
