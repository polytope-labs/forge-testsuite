## forge-testsuite

Some utilities for testing solidity contracts in rust.
For example, it might be useful to test cryptographic code in solidity from rust where generating the necessary
proofs is possible so it can then be verified by your solidity contracts. This uses forge internally, so your solidity project
should also be a foundry project as well.

Ensure that your test crate lives within the solidity project such that the solidity project is
one folder above your rust test crate. eg

```
solidity/
├── src/
├── lib/
├── scripts/
├── rust-test-crate/
│   ├── src/
│   ├── Cargo.toml
└── foundry.toml
```

## installation

add the following to your `Cargo.toml`

```toml
forge-testsuite = { git = "https://github.com/polytope-labs/forge-testsuite", branch = "master" }
```

## Usage

```rust
use forge_testsuite::Runner;


#[tokio::test]
async fn contract_tests() -> Result<(), anyhow::Error> {
    let mut runner = Runner::new();
    
    // print a list of all detected test contracts
    println!("{:?}", runner.contracts);
    
    let contract = runner.deploy("TestContract").await;
    
    let result: bool = contract.call("testMethod", ("arguments to the contract"))?;
    
    assert!(result);
    
    Ok(())
}

```