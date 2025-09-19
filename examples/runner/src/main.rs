use revm::primitives::U256;
use revmc_examples_runner::{build_evm, run_fibonacci, build_evm_normal};
use std::time::{Duration, Instant};

fn main() {
    let t_aot = fibonacci();
    let t_normal = fibonacci_normal();

    println!(
        "Timing: AOT compiled = {} ms, normal = {} ms (1..=100)",
        t_aot.as_millis(),
        //1,
        t_normal.as_millis(),
    );
}

fn fibonacci() -> Duration {
    let (mut evm, addr) = build_evm();
    let start = Instant::now();
    for i in 1..=500u64 {
        let actual_num = U256::from(i - 1);
        let out = run_fibonacci(&mut evm, addr, actual_num);
        println!("fib({i}) = {}", out);
    }
    let elapsed = start.elapsed();
    elapsed
}

fn fibonacci_normal() -> Duration {
    let (mut evm, addr) = build_evm_normal();

    let start = Instant::now();
    for i in 1..=500u64 {
        let actual_num = U256::from(i - 1);
        let out = run_fibonacci(&mut evm, addr, actual_num);
        println!("fib({i}) = {}", out);
    }
    let elapsed = start.elapsed();
    elapsed
}