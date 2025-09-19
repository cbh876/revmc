use core::ffi::c_void;
use revm_ffi_bridge::{FfiCallData, FfiEntryFn, FfiHostVTable, FfiReturnData, TRANSFER_SELECTOR};
use tiny_keccak::{Hasher, Keccak};

const BALANCES_SLOT: [u8; 32] = {
    let mut a = [0u8; 32];
    a[31] = 3; // slot = 3
    a
};

#[no_mangle]
pub unsafe extern "C" fn custom_revmc_ffi(
    host_ctx: *mut c_void,
    calldata: FfiCallData,
    ret: FfiReturnData,
    host: *const FfiHostVTable,
) -> i32 {
    let host = &*host;
    let input = core::slice::from_raw_parts(calldata.ptr, calldata.len);
    if input.len() < 4 { return -1; }
    let selector = &input[..4];
    if selector != &TRANSFER_SELECTOR { return -1; }

    // decode transfer(address,uint256)
    if input.len() < 4 + 32 + 32 { return -1; }
    let to = &input[4 + 12 .. 4 + 32];
    let amount = &input[4 + 32 .. 4 + 64];

    // caller
    let mut caller = [0u8; 20];
    if (host.get_caller)(host_ctx, caller.as_mut_ptr()) != 0 { return -1; }

    // compute storage key for balances[addr] => keccak(pad(addr)|pad(slot))
    fn key_for(addr20: &[u8; 20]) -> [u8; 32] {
        let mut buf = [0u8; 64];
        // pad(addr)
        buf[12..32].copy_from_slice(addr20);
        // pad(slot)
        buf[32..64].copy_from_slice(&BALANCES_SLOT);
        let mut out = [0u8; 32];
        let mut keccak = Keccak::v256();
        keccak.update(&buf);
        keccak.finalize(&mut out);
        out
    }

    let from_key = key_for(&caller);
    let mut from_bal = [0u8; 32];
    if (host.sload)(host_ctx, caller.as_ptr(), from_key.as_ptr(), from_bal.as_mut_ptr()) != 0 { return -1; }

    let mut to20 = [0u8; 20];
    to20.copy_from_slice(to);
    let to_key = key_for(&to20);
    let mut to_bal = [0u8; 32];
    if (host.sload)(host_ctx, to20.as_ptr(), to_key.as_ptr(), to_bal.as_mut_ptr()) != 0 { return -1; }

    // from_bal >= amount ?
    if !ge_u256_be(&from_bal, amount) { return write_bool(ret, false); }

    // from_bal -= amount; to_bal += amount
    let new_from = sub_u256_be(&from_bal, amount);
    let new_to = add_u256_be(&to_bal, amount);

    if (host.sstore)(host_ctx, caller.as_ptr(), from_key.as_ptr(), new_from.as_ptr()) != 0 { return -1; }
    if (host.sstore)(host_ctx, to20.as_ptr(), to_key.as_ptr(), new_to.as_ptr()) != 0 { return -1; }

    write_bool(ret, true)
}

fn write_bool(ret: FfiReturnData, v: bool) -> i32 {
    unsafe {
        if ret.cap < 32 { return -1; }
        let out = core::slice::from_raw_parts_mut(ret.ptr, ret.cap);
        for b in &mut out[..32] { *b = 0; }
        if v { out[31] = 1; }
        32
    }
}

fn ge_u256_be(a: &[u8; 32], b: &[u8]) -> bool {
    let mut bb = [0u8; 32];
    let start = 32 - b.len();
    bb[start..].copy_from_slice(b);
    a >= &bb
}

fn add_u256_be(a: &[u8; 32], b: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut carry = 0u16;
    for i in (0..32).rev() {
        let ai = a[i] as u16;
        let bi = if i >= 32 - b.len() { b[i - (32 - b.len())] as u16 } else { 0 };
        let sum = ai + bi + carry;
        out[i] = (sum & 0xff) as u8;
        carry = sum >> 8;
    }
    out
}

fn sub_u256_be(a: &[u8; 32], b: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut borrow = 0i16;
    for i in (0..32).rev() {
        let ai = a[i] as i16;
        let bi = if i >= 32 - b.len() { b[i - (32 - b.len())] as i16 } else { 0 };
        let mut diff = ai - bi - borrow;
        if diff < 0 { diff += 256; borrow = 1; } else { borrow = 0; }
        out[i] = (diff & 0xff) as u8;
    }
    out
}

