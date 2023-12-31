## forge-testsuite

Some utilities for testing solidity contracts in rust.
For example, it might be useful to test cryptographic code in solidity from rust where generating the necessary
proofs is possible so it can then be verified by your solidity contracts. This uses forge internally, so your solidity project
should also be a foundry project as well.


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
    let mut runner = Runner::new(PathBuf::from("/path/to/your/foundry/project"));
    
    // print a list of all detected test contracts
    println!("{:?}", runner.contracts);
    
    let contract = runner.deploy("TestContract").await;
    
    let result: bool = contract.call("testMethod", ("arguments to the contract"))?;
    
    assert!(result);
    
    Ok(())
}

```