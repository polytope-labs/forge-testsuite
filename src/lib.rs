// Copyright (C) 2023 Polytope Labs.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Some utilities for testing solidity contracts in rust.
//! It might be useful to test cryptographic code in solidity from rust generating the necessary
//! proofs to be then verified.
//!
//! Ensure that your test crate lives within the solidity project such that the solidity project is
//! one folder above your rust test crate. eg
//! solidity/
//! ├── src/
//! ├── lib/
//! ├── scripts/
//! ├── rust-test-crate/
//! │   ├── src/
//! │   ├── Cargo.toml
//! └── foundry.toml

use ethers::{
    abi::{Detokenize, Tokenize},
    solc::{remappings::Remapping, Project, ProjectCompileOutput, ProjectPathsConfig},
    types::{Log, U256},
};
use forge::{
    executor::{
        inspector::CheatsConfig,
        opts::{Env, EvmOpts},
    },
    result::TestSetup,
    ContractRunner, MultiContractRunner, MultiContractRunnerBuilder,
};
use foundry_config::{fs_permissions::PathPermission, Config, FsPermissions};
use foundry_evm::{
    decode::decode_console_logs,
    executor::{Backend, EvmError, ExecutorBuilder},
    Address,
};
use once_cell::sync::Lazy;
use std::{
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};

static PROJECT: Lazy<Project> = Lazy::new(|| {
    // root should be configurable
    let root = manifest_root();
    let mut paths = ProjectPathsConfig::builder().root(root.clone()).build().unwrap();

    // parse remappings from remappings.txt.
    fs::read_to_string(root.clone().join("remappings.txt"))
        .unwrap()
        .lines()
        .map(|line| {
            let iter = line.split("=").collect::<Vec<_>>();
            Remapping {
                context: None,
                name: iter[0].to_string(),
                path: root
                    .clone()
                    .join(&iter[1].to_string())
                    .into_os_string()
                    .into_string()
                    .unwrap(),
            }
        })
        .for_each(|mapping| {
            paths.remappings.retain(|m| m.name != mapping.name);
            paths.remappings.push(mapping)
        });

    Project::builder().paths(paths).build().unwrap()
});

static EVM_OPTS: Lazy<EvmOpts> = Lazy::new(|| EvmOpts {
    env: Env {
        gas_limit: 18446744073709551615,
        chain_id: Some(foundry_common::DEV_CHAIN_ID),
        tx_origin: Config::DEFAULT_SENDER,
        block_number: 1,
        block_timestamp: 1,
        code_size_limit: Some(usize::MAX),
        ..Default::default()
    },
    sender: Config::DEFAULT_SENDER,
    initial_balance: U256::MAX,
    ffi: true,
    memory_limit: 2u64.pow(24),
    ..Default::default()
});

static COMPILED: Lazy<ProjectCompileOutput> = Lazy::new(|| {
    let out = (*PROJECT).compile().unwrap();
    if out.has_compiler_errors() {
        eprintln!("{out}");
        panic!("Compiled with errors");
    }
    out
});

/// Builds a base runner
fn base_runner() -> MultiContractRunnerBuilder {
    MultiContractRunnerBuilder::default().sender(EVM_OPTS.sender)
}

fn manifest_root() -> PathBuf {
    let mut root = Path::new(env!("CARGO_MANIFEST_DIR"));

    root = root.parent().unwrap();
    root.to_path_buf()
}

/// Builds a non-tracing runner
fn runner_with_config(mut config: Config) -> MultiContractRunner {
    use foundry_evm::executor::SpecId;

    config.allow_paths.push(manifest_root());

    base_runner()
        .with_cheats_config(CheatsConfig::new(&config, &EVM_OPTS))
        .evm_spec(SpecId::MERGE)
        .sender(config.sender)
        .build(&PROJECT.paths.root, (*COMPILED).clone(), EVM_OPTS.local_evm_env(), EVM_OPTS.clone())
        .unwrap()
}

/// The contract runner. Use this to deploy contracts for executing.
pub struct Runner {
    runner: MultiContractRunner,
}

impl AsRef<MultiContractRunner> for Runner {
    fn as_ref(&self) -> &MultiContractRunner {
        &self.runner
    }
}

impl AsMut<MultiContractRunner> for Runner {
    fn as_mut(&mut self) -> &mut MultiContractRunner {
        &mut self.runner
    }
}

impl Runner {
    /// Builds a non-tracing runner
    pub fn new() -> Self {
        let mut config = Config::with_root(PROJECT.root());
        config.fs_permissions =
            FsPermissions::new(vec![PathPermission::read_write(manifest_root())]);
        Self { runner: runner_with_config(config) }
    }

    /// Deploy a contract with the provided name and return a handle for executing it's methods.
    pub async fn deploy<'a>(&'a mut self, contract_name: &'static str) -> Contract<'a> {
        let runner = &mut self.runner;

        let (id, (abi, deploy_code, libs)) = runner
            .contracts
            .iter()
            .find(|(id, (_, _, _))| id.name == contract_name)
            .unwrap();

        // dbg!(deploy_code.len());
        // dbg!(2 * 0x6000); // max init codesize

        let db = Backend::spawn(runner.fork.take()).await;
        let executor = ExecutorBuilder::default()
            .with_cheatcodes(runner.cheats_config.clone())
            .with_config(runner.env.clone())
            .with_spec(runner.evm_spec)
            .with_gas_limit(runner.evm_opts.gas_limit())
            .set_tracing(true)
            .set_coverage(runner.coverage)
            .build(db.clone());

        let mut single_runner = ContractRunner::new(
            &id.name,
            executor,
            abi,
            deploy_code.clone(),
            runner.evm_opts.initial_balance,
            runner.sender,
            runner.errors.as_ref(),
            libs,
        );

        let setup = single_runner.setup(true);
        let TestSetup { address, .. } = setup;

        Contract { runner: single_runner, address }
    }
}

/// Handle for executing a single Contract.
pub struct Contract<'a> {
    /// The contract runner
    runner: ContractRunner<'a>,
    /// The contract address
    address: Address,
}

impl<'a> Contract<'a> {
    pub async fn call<T, R>(&mut self, func: &'static str, args: T) -> Result<R, EvmError>
    where
        T: Tokenize,
        R: Detokenize + Debug,
    {
        let contract = &mut self.runner;
        let function = contract.contract.functions.get(func).unwrap().first().unwrap().clone();

        let result = contract.executor.execute_test::<R, _, _>(
            contract.sender,
            self.address,
            function,
            args,
            0.into(),
            contract.errors,
        );

        match &result {
            Ok(call) => print_logs(func, call.gas_used, &call.logs),
            Err(EvmError::Execution(execution)) =>
                print_logs(func, execution.gas_used, &execution.logs),
            _ => {},
        };

        Ok(result?.result)
    }
}

fn print_logs(func: &str, gas_used: u64, logs: &Vec<Log>) {
    println!("Gas used {func}: {:#?}", gas_used);
    println!("=========== Start Logs {func} ===========");
    for log in decode_console_logs(logs) {
        println!("{}", log);
    }
    println!("=========== End Logs {func} ===========");
}
