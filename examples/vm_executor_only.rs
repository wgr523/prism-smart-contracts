
use log::{debug, error, info};
use prism::api::Server as ApiServer;
use prism::blockchain::BlockChain;
use prism::blockdb::BlockDatabase;
use prism::config::BlockchainConfig;
use prism::crypto::hash::{H256, Address};
use prism::wallet::Wallet;
use std::convert::TryInto;
use std::sync::Arc;
use std::time::Instant;
use std::io;
use std::collections::HashSet;

use prism::statedb::StateDatabase;
use rand::Rng;
use rand::distributions::WeightedIndex;
use rand::distributions::Distribution;
use rand::seq::SliceRandom;

fn main() {
    stderrlog::new().verbosity(2).init().unwrap();
    run_local(String::from("payment"),10000,100000,200);
    run_local(String::from("donothing"),10000,100000,200);
    run_local(String::from("simpleerc20"),10000,100000,200);
    run_local(String::from("cryptokitties"),10000,100000,200);
    run_local(String::from("cpuheavy"),10000,100000,200);
    run_local(String::from("ioheavy"),10000,100000,200);
}

fn run_local(transaction_type: String, key_num: usize, tx_num: usize, tx_per_block: usize) {
    macro_rules! include_int_vec {
        ($file: expr) => {{
            let my_str = include_str!($file);
            my_str.lines().map(|x|x.parse::<u64>().expect("experiment/*.txt format error.")).collect()
        }};
    }


    let mut rng = rand::thread_rng();

    let statedb = StateDatabase::new("/tmp/prism/node_0-utxodb.rocksdb").unwrap();
    let statedb = Arc::new(statedb);

    let wallet = Wallet::new("/tmp/prism/node_0-wallet.rocksdb").unwrap();
    let wallet = Arc::new(wallet);

    let mut addrs = vec![];
    for i in 0..key_num {
        addrs.push(wallet.generate_keypair().unwrap());
    }

    let probs: Vec<u64> = include_int_vec!("../src/experiment/from_address_counter.txt");
    let probs: Vec<u64> = probs.into_iter().take(addrs.len()).collect();
    let from_dist = WeightedIndex::new(&probs).unwrap();
    let probs: Vec<u64> = include_int_vec!("../src/experiment/to_address_counter.txt");
    let mut probs: Vec<u64> = probs.into_iter().take(addrs.len()).collect();
    probs.shuffle(&mut rng);
    let to_dist = WeightedIndex::new(&probs).unwrap();

    prism::experiment::ico(&addrs, &statedb, &wallet, 0xffffffffffffffff).unwrap();

    let mut has_erc20_ico: HashSet<Address> = HashSet::new();

    let mut txs = vec![];
    //let mut addr_iter = addrs.iter().cycle();
    for _ in 0..tx_num {
        let addr = addrs.get(from_dist.sample(&mut rng)).unwrap();
        let to_addr = addrs.get(to_dist.sample(&mut rng)).unwrap();
        let transaction = match transaction_type.trim() {
            "payment" => wallet.create_transaction_payment(addr, to_addr, 1.into()),
            "donothing" => wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xf01), hex::decode("448f30a3").unwrap()),
            "cpuheavy" => wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xf02), hex::decode("fe91386500000000000000000000000000000000000000000000000000000000000000ff").unwrap()),
            "ioheavy" => wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xf03), hex::decode("b6a3f0e900000000000000000000000000000000000000000000000000000000000000ff").unwrap()),
            "cryptokitties" => {
                let mut data = hex::decode("0d9f5aed").unwrap();
                // generate 2 random u256 parameter (= 4 u128)
                for _ in 0..4 {
                    let t: u128 = rng.gen();
                    data.extend_from_slice(t.to_be_bytes().as_ref());
                }
                // last parameter, target block number is 0
                data.append(&mut hex::decode("0000000000000000000000000000000000000000000000000000000000000000").unwrap());
                wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xe01), data)
            }
            "simpleerc20" => {
                if has_erc20_ico.contains(addr) {
                    // transfer tokens to self
                    let mut data = hex::decode("a9059cbb000000000000000000000000").unwrap();
                    data.extend_from_slice(to_addr.as_bytes());
                    data.append(&mut hex::decode("0000000000000000000000000000000000000000000000000000000000000001").unwrap());
                    wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xe02), data)
                } else {
                    has_erc20_ico.insert(addr.clone());
                    // mint tokens, number is very large: 0xffffffffffffffff
                    wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xe02), hex::decode("a0712d68000000000000000000000000000000000000000000000000ffffffffffffffff").unwrap())
                }
            }
            _ => panic!("Transaction type does not match any known one."),
        };
        txs.push(transaction.unwrap());
    }
    let start = Instant::now();

    for (i,tx) in txs.iter().enumerate() {
        statedb.apply(tx).unwrap();
        if (i+1) % tx_per_block == 0 {
            statedb.commit().unwrap();
        }
    }
    let end = Instant::now();
    let time_1 = end.duration_since(start).as_micros() as f64;
    println!("Tx type {}", transaction_type);
    println!("Total {} txs", tx_num);
    println!("{} txs per block (block means commit)", tx_per_block);
    println!("Throughput: {} tps", tx_num as f64/time_1 *1000_000.0);
}
