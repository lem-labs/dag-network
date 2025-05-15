#[no_mangle]
pub extern "C" fn transfer() {
    let msg = "Transfer executed";
    unsafe {
        print_log(msg.as_ptr(), msg.len());
    }
}

#[no_mangle]
pub extern "C" fn balance_of() {
    let msg = "BalanceOf executed";
    unsafe {
        print_log(msg.as_ptr(), msg.len());
    }
}

extern "C" {
    fn print_log(ptr: *const u8, len: usize);
}
