//! WBNB 示例：使用 InlineAotEvm + FFI AOT 执行 transfer，并与正常解释执行对比；最终校验余额。

use std::time::{Duration, Instant};
use std::{env, fs, path::PathBuf};

use revm::{
	Context,
	context::FrameStack,
	database::InMemoryDB,
	handler::{
		inline_aot::InlineAotEvm,
		instructions::EthInstructions,
		AotPrecompiles, EthFrame, EthPrecompiles, ExecuteEvm, MainnetContext,ExecuteCommitEvm,
	},
	interpreter::interpreter::EthInterpreter,
	primitives::{address, hex, keccak256, Address, Bytes, TxKind, U256, B256},
	state::{AccountInfo, Bytecode},
};
use revm::context_interface::{ContextTr, Host};

// ---------------- Main ----------------
fn main() {
	let (code, code_hash) = load_bytecode();
	let mut args = std::env::args().skip(1);
	let iters: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);
	let to: Address = args
		.next()
		.and_then(|s| hex::decode(strip_0x(&s)).ok())
		.and_then(|v| if v.len()==20 { Some(Address::from_slice(&v)) } else { None })
		.unwrap_or(address!("000000000000000000000000000000000000BEEF"));
	let amount: U256 = args.next().and_then(|s| s.parse().ok()).unwrap_or(U256::from(1));

	let (t_aot, ok_aot) = bench_aot(&code, code_hash, iters, to, amount);
	let (t_normal, ok_normal) = bench_normal(&code, code_hash, iters, to, amount);

	println!(
		"Timing: AOT = {} ms, normal = {} ms (iters={}), verify: aot={}, normal={}",
		t_aot.as_millis(),
        //1,
		t_normal.as_millis(),
		iters,
		ok_aot,
		ok_normal
        //1,
	);
}

// --------------- Bench Routines ---------------
fn bench_aot(code: &[u8], code_hash: B256, iters: u64, to: Address, amount: U256) -> (Duration, bool) {
	let (mut evm, contract, caller) = build_evm_aot(code, code_hash);
	init_balance_slot3(&mut evm, contract, caller, amount.saturating_mul(U256::from(iters)));
	let start = Instant::now();
	let mut nonce = 0u64;
    for _ in 0..iters {
        assert!(call_transfer(&mut evm, contract, to, amount, nonce));
        nonce += 1;
    }
	let elapsed = start.elapsed();
	let ok = verify_transfer_result(&mut evm, contract, caller, to, amount.saturating_mul(U256::from(iters)));
	(elapsed, ok)
}

fn bench_normal(code: &[u8], code_hash: B256, iters: u64, to: Address, amount: U256) -> (Duration, bool) {
    let (mut evm, contract, caller) = build_evm_normal(code, code_hash);
    init_balance_slot3_normal(&mut evm, contract, caller, amount.saturating_mul(U256::from(iters)));
    let start = Instant::now();
    let mut nonce = 0u64;
    for _ in 0..iters {
        assert!(call_transfer_normal(&mut evm, contract, to, amount, nonce));
        nonce += 1;
    }
    let elapsed = start.elapsed();
    let ok = verify_transfer_result_normal(&mut evm, contract, caller, to, amount.saturating_mul(U256::from(iters)));
    (elapsed, ok)
}

fn verify_transfer_result(evm: &mut EvmTy, contract: Address, caller: Address, to: Address, moved: U256) -> bool {
	let caller_bal = sload_balance_slot3(evm, contract, caller);
	let to_bal = sload_balance_slot3(evm, contract, to);
	caller_bal == U256::ZERO && to_bal == moved
}

fn verify_transfer_result_normal(evm: &mut EvmTyNormal, contract: Address, caller: Address, to: Address, moved: U256) -> bool {
    let caller_bal = sload_balance_slot3_normal(evm, contract, caller);
    let to_bal = sload_balance_slot3_normal(evm, contract, to);
    caller_bal == U256::ZERO && to_bal == moved
}

// --------------- Build EVMs ---------------

type CtxT = MainnetContext<InMemoryDB>;
type InnerEvm = revm::context::Evm<CtxT, (), EthInstructions<EthInterpreter, CtxT>, AotPrecompiles<EthPrecompiles>, EthFrame<EthInterpreter>>;
type EvmTy = InlineAotEvm<CtxT, (), EthInstructions<EthInterpreter, CtxT>, AotPrecompiles<EthPrecompiles>, EthFrame<EthInterpreter>>;
type EvmTyNormal = InnerEvm;

fn build_evm_aot(code: &[u8], code_hash: B256) -> (EvmTy, Address, Address) {
	let ctx = Context::new(InMemoryDB::default(), revm::primitives::hardfork::SpecId::default());
	let so = resolve_aot_path();
	let inner: InnerEvm = revm::context::Evm {
		ctx,
		inspector: (),
		instruction: EthInstructions::new_mainnet(),
		precompiles: AotPrecompiles::new(EthPrecompiles::default(), B256::ZERO, PathBuf::new()),
		frame_stack: FrameStack::new(),
	};
	let mut evm: EvmTy = InlineAotEvm::from_evm(inner, |_interp, _ctx| { None })
		.with_ffi(so, b"custom_revmc_ffi");
	let contract = address!("0000000000000000000000000000000000009999");
	insert_code(&mut evm, contract, code, code_hash);
	let caller = revm::database::BENCH_CALLER;
	(evm, contract, caller)
}

fn build_evm_normal(code: &[u8], code_hash: B256) -> (EvmTyNormal, Address, Address) {
	let ctx = Context::new(InMemoryDB::default(), revm::primitives::hardfork::SpecId::default());
	let mut evm: InnerEvm = revm::context::Evm {
		ctx,
		inspector: (),
		instruction: EthInstructions::new_mainnet(),
		precompiles: AotPrecompiles::new(EthPrecompiles::default(), B256::ZERO, PathBuf::new()),
		frame_stack: FrameStack::new(),
	};
	let contract = address!("0000000000000000000000000000000000009999");
	insert_code_normal(&mut evm, contract, code, code_hash);
	let caller = revm::database::BENCH_CALLER;
	(evm, contract, caller)
}

fn insert_code(evm: &mut EvmTy, addr: Address, code: &[u8], code_hash: B256) {
	let mut info = AccountInfo::default();
	let b = Bytecode::new_raw(Bytes::copy_from_slice(code));
	info.set_code_and_hash(b, code_hash);
	evm.inner.ctx.db_mut().insert_account_info(addr, info);
    // Ensure account is loaded into journal to avoid unwrap panic on first sload/sstore
    let _ = evm.inner.ctx.load_account_info_skip_cold_load(addr, true, false);
}

fn insert_code_normal(evm: &mut EvmTyNormal, addr: Address, code: &[u8], code_hash: B256) {
    let mut info = AccountInfo::default();
    let b = Bytecode::new_raw(Bytes::copy_from_slice(code));
    info.set_code_and_hash(b, code_hash);
    evm.ctx.db_mut().insert_account_info(addr, info);
    let _ = evm.ctx.load_account_info_skip_cold_load(addr, true, false);
}

// --------------- Calls ---------------

fn call_transfer(evm: &mut EvmTy, addr: Address, to: Address, amount: U256, nonce: u64) -> bool {
    let data = encode_transfer(to, amount);
    let tx = revm::context::TxEnv::builder()
        .caller(revm::database::BENCH_CALLER)
        .kind(TxKind::Call(addr))
        .data(data)
        .gas_limit(30_000_000)
        .nonce(nonce) // 关键：设置本次交易的 nonce
        .build()
        .unwrap();
    let result = evm.transact_commit(tx).unwrap();
    let out = result.output().cloned().unwrap_or_default();
    out.len() == 32 && out[31] == 1
}

fn call_transfer_normal(evm: &mut EvmTyNormal, addr: Address, to: Address, amount: U256, nonce: u64) -> bool {
    let data = encode_transfer(to, amount);
    let tx = revm::context::TxEnv::builder()
        .caller(revm::database::BENCH_CALLER)
        .kind(TxKind::Call(addr))
        .data(data)
        .gas_limit(30_000_000)
        .nonce(nonce)
        .build()
        .unwrap();
    let result = ExecuteCommitEvm::transact_commit(evm, tx).unwrap();
    let out = result.output().cloned().unwrap_or_default();
    out.len() == 32 && out[31] == 1
}

// --------------- Storage helpers (slot=3) ---------------

fn map_key_slot3(addr: Address) -> B256 {
	let mut buf = [0u8; 64];
	buf[12..32].copy_from_slice(addr.as_slice());
	buf[60..64].copy_from_slice(&3u32.to_be_bytes());
	keccak256(buf)
}

fn sload_balance_slot3(evm: &mut EvmTy, contract: Address, who: Address) -> U256 {
    let key = U256::from_be_slice(map_key_slot3(who).as_slice());
    // 关键：先把合约账户加载进 journal，避免 unwrap(None)
    let _ = evm.inner.ctx.load_account_info_skip_cold_load(contract, true, false);
    evm.inner
        .ctx
        .sload(contract, key)
        .map(|l| l.data)
        .unwrap_or(U256::ZERO)
}

fn sload_balance_slot3_normal(evm: &mut EvmTyNormal, contract: Address, who: Address) -> U256 {
    let key = U256::from_be_slice(map_key_slot3(who).as_slice());
    let _ = evm.ctx.load_account_info_skip_cold_load(contract, true, false);
    evm.ctx
        .sload(contract, key)
        .map(|l| l.data)
        .unwrap_or(U256::ZERO)
}

fn init_balance_slot3(evm: &mut EvmTy, contract: Address, who: Address, amount: U256) {
    let key = U256::from_be_slice(map_key_slot3(who).as_slice());
    let _ = evm.inner.ctx.sstore(contract, key, amount);
}

fn init_balance_slot3_normal(evm: &mut EvmTyNormal, contract: Address, who: Address, amount: U256) {
    let key = U256::from_be_slice(map_key_slot3(who).as_slice());
    let _ = evm.ctx.sstore(contract, key, amount);
}

// --------------- Utilities ---------------

fn resolve_aot_path() -> PathBuf {
	if let Ok(p) = env::var("AOT_WBNB_PATH") { let pb = PathBuf::from(p); if pb.exists() { return pb; } }
	let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target");
	let name = if cfg!(target_os = "macos") { "libaot_wbnb_ffi.dylib" }
		else if cfg!(target_os = "windows") { "aot_wbnb_ffi.dll" }
		else { "libaot_wbnb_ffi.so" };
	for prof in ["debug", "release"] { for sub in ["", "deps"] {
		let p = if sub.is_empty() { root.join(prof).join(name) } else { root.join(prof).join(sub).join(name) };
		if p.exists() { return p; }
	}}
	root.join("release").join(name)
}

fn load_bytecode() -> (Vec<u8>, B256) {
	let bin_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wbnb.runtime.bin");
	let data = fs::read(bin_path).expect("read wbnb.runtime.bin");
	let hash: revm::primitives::FixedBytes<32> = revm::bytecode::Bytecode::new_raw(Bytes::copy_from_slice(&data)).hash_slow();
	(data, hash)
}

fn encode_transfer(to: Address, amount: U256) -> Bytes {
	const SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];
	let mut data = Vec::with_capacity(4 + 32 + 32);
	data.extend_from_slice(&SELECTOR);
	data.extend_from_slice(&[0u8; 12]);
	data.extend_from_slice(to.as_slice());
	let be = amount.to_be_bytes_vec();
	data.extend_from_slice(&be);
	Bytes::from(data)
}

fn strip_0x(s: &str) -> &str { s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s) }