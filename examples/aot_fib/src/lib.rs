#[export_name = "custom"]
pub extern "C" fn entry(inp_ptr: *const u8, inp_len: usize, out_ptr: *mut u8, out_cap: usize) -> i32 {
    // 读取 32 字节大端 U256，解释为 n
    let input = unsafe { core::slice::from_raw_parts(inp_ptr, inp_len) };
    let mut buf = [0u8; 32];
    if input.len() <= 32 { buf[32 - input.len()..].copy_from_slice(input); } else { buf.copy_from_slice(&input[input.len() - 32..]); }

    // 计算 fib(n+1)（与 runner 的合约约定一致）
    let n = u128::from_be_bytes(buf[16..32].try_into().unwrap());
    let target = n.saturating_add(1);
    let mut a: u128 = 0;
    let mut b: u128 = 1;
    for _ in 0..target { let t = a.saturating_add(b); a = b; b = t; }
    let val = a;

    // 写回 32 字节大端结果
    let out = unsafe { core::slice::from_raw_parts_mut(out_ptr, out_cap) };
    let mut res = [0u8; 32];
    res[16..32].copy_from_slice(&val.to_be_bytes());
    let n = core::cmp::min(32, out.len());
    out[..n].copy_from_slice(&res[..n]);
    n as i32
}

