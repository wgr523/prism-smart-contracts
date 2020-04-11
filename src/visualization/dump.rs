use crate::blockchain::BlockChain;
use crate::blockdb::BlockDatabase;
use crate::statedb::StateDatabase;
use crate::crypto::hash::{Address, EthereumH256, H256};
use ethereum_types::U256;

#[derive(Serialize)]
pub struct Dump {
    /// Ordered tx blocks
    pub transactions_blocks: Vec<H256>,
}

pub fn dump_ledger(
    blockchain: &BlockChain,
    _blockdb: &BlockDatabase,
    limit: u64,
) -> String {
    let ledger = match blockchain.proposer_transaction_in_ledger(limit) {
        Err(_) => return "database err".to_string(),
        Ok(v) => v,
    };

    let mut transactions_blocks: Vec<H256> = vec![];
    // loop over all tx blocks in the ledger
    for (proposer_hash, mut tx_block_hashes) in ledger {
        transactions_blocks.append(&mut tx_block_hashes);
    }
    let dump = Dump {
        transactions_blocks,
    };
    serde_json::to_string_pretty(&dump).unwrap()
}

pub fn dump_voter_timestamp(blockchain: &BlockChain, blockdb: &BlockDatabase) -> String {
    let proposer_bottom_tip =
        blockchain
            .proposer_bottom_tip()
            .unwrap_or((H256::default(), H256::default(), 0));
    let voter_bottom_tip = blockchain.voter_bottom_tip().unwrap_or(vec![]);
    let mut dump = vec![];
    let bottom_timestamp = match blockdb.get(&proposer_bottom_tip.0).unwrap_or(None) {
        Some(block) => block.header.timestamp,
        _ => 0,
    };
    let tip_timestamp = match blockdb.get(&proposer_bottom_tip.1).unwrap_or(None) {
        Some(block) => block.header.timestamp,
        _ => 0,
    };
    if proposer_bottom_tip.2 > 1 && tip_timestamp != bottom_timestamp {
        dump.push(format!(
            "Proposer tree, {:6.3} s / {:3} level = {:10.3}",
            (tip_timestamp - bottom_timestamp) as f64 / 1000f64,
            proposer_bottom_tip.2 - 1,
            (tip_timestamp - bottom_timestamp) as f64
                / (proposer_bottom_tip.2 - 1) as f64
                / 1000f64
        ));
    } else {
        dump.push("Proposer tree only grows zero or one level.".to_string());
    }
    for (chain, (bottom, tip, level)) in voter_bottom_tip.iter().enumerate() {
        let bottom_timestamp = match blockdb.get(bottom).unwrap_or(None) {
            Some(block) => block.header.timestamp,
            _ => 0,
        };
        let tip_timestamp = match blockdb.get(tip).unwrap_or(None) {
            Some(block) => block.header.timestamp,
            _ => 0,
        };
        if *level > 1 && tip_timestamp != bottom_timestamp {
            dump.push(format!(
                "Chain {:7}, {:6.3} s / {:3} level = {:10.3}",
                chain,
                (tip_timestamp - bottom_timestamp) as f64 / 1000f64,
                *level - 1,
                (tip_timestamp - bottom_timestamp) as f64 / (*level - 1) as f64 / 1000f64
            ));
        } else {
            dump.push(format!("Chain {:7} only grows zero or one level.", chain));
        }
    }
    serde_json::to_string_pretty(&dump).unwrap()
}

#[derive(Serialize)]
struct AccountDump {
    address: Address,
    balance: U256,
    nonce: U256,
    code: Option<Vec<u8>>,
    key: Option<EthereumH256>,
    value: Option<EthereumH256>,
}

pub fn dump_account(statedb: &StateDatabase, address: &Address, key: &Option<EthereumH256>) -> String {
    let dump = AccountDump {
        address: address.clone(),
        balance: statedb.balance(address).unwrap(),
        nonce: statedb.nonce(address).unwrap(),
        code: statedb.code(address).unwrap().map(|arc|(*arc).clone()),
        key: key.clone(),
        value: key.and_then(|ref k|statedb.storage_at(address, k).ok()),
    };
    serde_json::to_string_pretty(&dump).unwrap()
}
