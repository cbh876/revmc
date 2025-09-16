//! 基于本地 revm 的最小运行器：按字节码哈希拦截，直接计算 Fibonacci。

use revm::{
    Context,
    context::FrameStack,
    database::InMemoryDB,
    handler::{
        instructions::EthInstructions,
        AotPrecompiles, EthFrame, EthPrecompiles, ExecuteEvm, MainnetContext,
    },
    interpreter::interpreter::EthInterpreter,
    primitives::{hex, Address, Bytes, TxKind, U256, B256},
    state::{AccountInfo, Bytecode},
};
use revm::context_interface::ContextTr;
use std::{env, path::{Path, PathBuf}};

include!("./common.rs");

type CtxT = MainnetContext<InMemoryDB>;
#[allow(type_alias_bounds)]
type EvmTy = revm::context::Evm<
    CtxT,
    (),
    EthInstructions<EthInterpreter, CtxT>,
    AotPrecompiles<EthPrecompiles>,
    EthFrame<EthInterpreter>,
>;

/// 构建一个 mainnet EVM，并把 Fibonacci 合约代码和哈希插入到内存数据库。
pub fn build_evm() -> (EvmTy, Address) {
    let ctx = Context::new(
        InMemoryDB::default(),
        revm::primitives::hardfork::SpecId::default(),
    );

    // 用 AOT 动态库包裹 precompiles：命中指定 code_hash 时，调用动态库 `custom`
    let so_path = resolve_aot_lib_path();

    let mut evm: EvmTy = revm::context::Evm {
        ctx,
        inspector: (),
        instruction: EthInstructions::new_mainnet(),
        precompiles: AotPrecompiles::new(EthPrecompiles::default(), FIBONACCI_HASH.into(), so_path),
        frame_stack: FrameStack::new(),
    };

    // 部署地址和账户信息
    let addr = Address::from_slice(&hex::decode("0000000000000000000000000000000000001234").unwrap());
    let mut info = AccountInfo::default();
    let code = Bytecode::new_raw(Bytes::from_static(FIBONACCI_CODE));
    // 保持 hash 与字节码一致（示例中也给出固定 hash）
    info.set_code_and_hash(code, FIBONACCI_HASH.into());
    evm.ctx.db_mut().insert_account_info(addr, info);

    (evm, addr)
}



fn resolve_aot_lib_path() -> PathBuf {
    // 1) 允许外部通过环境变量覆盖
    if let Ok(p) = env::var("AOT_FIB_PATH") {
        let pb = PathBuf::from(p);
        if pb.exists() { return pb; }
    }

    // 2) 推导 workspace target 目录（优先 CARGO_TARGET_DIR，其次 workspace 根的 target）
    let runner_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = runner_dir.parent().and_then(Path::parent).unwrap_or(runner_dir);
    let target_dir = env::var("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|_| workspace_root.join("target"));

    let lib_name = if cfg!(target_os = "macos") {
        "libaot_fib.dylib"
    } else if cfg!(target_os = "windows") {
        "aot_fib.dll"
    } else {
        "libaot_fib.so"
    };

    // 3) 根据当前构建配置优先级选择 profile：debug 运行优先 debug，否则 release 优先
    let profiles: [&str; 2] = if cfg!(debug_assertions) { ["debug", "release"] } else { ["release", "debug"] };

    for prof in profiles {
        let p1 = target_dir.join(prof).join(lib_name);
        if p1.exists() { return p1; }
        let p2 = target_dir.join(prof).join("deps").join(lib_name);
        if p2.exists() { return p2; }
    }

    // 4) 最后回退为 release 顶层路径，便于错误提示
    target_dir.join("release").join(lib_name)
}

/// 计算 fib(n)，n 为交易 data 的大端 U256 值，合约逻辑是 fib(input+1)，与原示例一致。
pub fn run_fibonacci(evm: &mut EvmTy, addr: Address, n: U256) -> U256 {
    let tx = revm::context::TxEnv::builder()
        .kind(TxKind::Call(addr))
        .data(Bytes::from(n.to_be_bytes_vec()))
        .gas_limit(1_000_000)
        .build()
        .unwrap();

    let result = evm.transact(tx).unwrap();
    let out = result.result.output().unwrap();
    U256::from_be_slice(out)
}

pub fn build_evm_normal() -> (EvmTy, Address) {
    let ctx = Context::new(
        InMemoryDB::default(),
        revm::primitives::hardfork::SpecId::default(),
    );

    let mut evm: EvmTy = revm::context::Evm {
        ctx,
        inspector: (),
        instruction: EthInstructions::new_mainnet(),
        precompiles: AotPrecompiles::new(EthPrecompiles::default(), B256::ZERO, PathBuf::new()),
        frame_stack: FrameStack::new(),
    };

    // 部署地址和账户信息
    let addr = Address::from_slice(&hex::decode("0000000000000000000000000000000000001234").unwrap());
    let mut info = AccountInfo::default();
    let code = Bytecode::new_raw(Bytes::from_static(FIBONACCI_CODE));
    // 保持 hash 与字节码一致（示例中也给出固定 hash）
    info.set_code_and_hash(code, FIBONACCI_HASH.into());
    evm.ctx.db_mut().insert_account_info(addr, info);

    (evm, addr)
}