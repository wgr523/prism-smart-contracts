# Prism removes consensus bottleneck for smart contracts

## Paper

__Prism Removes Consensus Bottleneck for Smart Contracts__ [\[full text\]](http://arxiv.org/abs/2004.08776)

_Gerui Wang, Shuo Wang, Vivek Bagaria, David Tse, Pramod Viswanath_

Abstract: The performance of existing permissionless smart contract platforms such as Ethereum is limited by the consensus layer. Prism is a new proof-of-work consensus protocol that provably achieves throughput and latency up to physical limits while retaining the strong guarantees of the longest chain protocol. This paper reports experimental results from implementations of two smart contract virtual machines, EVM and MoveVM, on top of Prism and demonstrates that the consensus bottleneck has been removed.

## Build

This project requires Rust `nightly`. To build the binary, run `cargo build --release`.

The first build could take several mintues, mostly due to building dependencies from Ethereum.

## Reproducing EVM Prism results

The scripts used in the evaluation section of the paper are located in `/testbed`. `/testbed/README.md` provides instructions for running the experiments and reproducing the results.

## Reproducing EVM Executor Only results

Simply run `cargo run --release --example vm_executor_only`.

## Acknowledgement

This repository is forked from [Prism: Scaling Bitcoin by 10,000x](https://github.com/yangl1996/prism-rust) and [Parity Ethereum](https://github.com/openethereum/openethereum).
