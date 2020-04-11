use crate::block::Content;
use crate::blockchain::BlockChain;
use crate::blockdb::BlockDatabase;
use crate::crypto::hash::{Hashable, H256, Address};
use crate::experiment::performance_counter::PERFORMANCE_COUNTER;
use log::{trace, debug, info, warn};
use crate::transaction::{Transaction, UnverifiedTransaction};
use crate::config::LAZY_ANNOTATION;

use crate::statedb::StateDatabase;
use crate::wallet::Wallet;
use crossbeam::channel;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::thread;

pub struct LedgerManager {
    blockdb: Arc<BlockDatabase>,
    chain: Arc<BlockChain>,
    statedb: Arc<StateDatabase>,
    wallet: Arc<Wallet>,
}

impl LedgerManager {
    pub fn new(
        blockdb: &Arc<BlockDatabase>,
        chain: &Arc<BlockChain>,
        statedb: &Arc<StateDatabase>,
        wallet: &Arc<Wallet>,
    ) -> Self {
        Self {
            blockdb: Arc::clone(&blockdb),
            chain: Arc::clone(&chain),
            statedb: Arc::clone(&statedb),
            wallet: Arc::clone(&wallet),
        }
    }

    pub fn start(self, buffer_size: usize) {
        // start thread that updates transaction sequence
        let blockdb = Arc::clone(&self.blockdb);
        let chain = Arc::clone(&self.chain);
        let statedb = Arc::clone(&self.statedb);
        let (tx_diff_tx, tx_diff_rx) = channel::bounded(buffer_size);
        thread::spawn(move || loop {
            update_transaction_sequence(&blockdb, &chain, &tx_diff_tx);
        });

        thread::spawn(move || {
            loop {
                // get the diff
                let added_tx = tx_diff_rx.recv().unwrap();

                for tx in added_tx {
                    let outcome = statedb.apply(&tx).unwrap();
                    PERFORMANCE_COUNTER.record_confirm_transaction(&tx);
                    /*
                    // try to get address if it's create contract, useful when debugging
                    let contract_addr = match outcome.trace.get(0).map(|trace|&trace.result) {
                    Some(trace::trace::Res::Create(res)) => Some(res.address.clone()),
                    _ => None,
                    };
                    if let Some(addr) = contract_addr {
                    info!("A new contract {:?} is created.", addr);
                    }
                    */
                    // check if execution is successful
                    match outcome.receipt.outcome {
                        common_types::receipt::TransactionOutcome::StatusCode(x) if x==1 => {}
                        common_types::receipt::TransactionOutcome::StatusCode(x) if x==0 => {
                            warn!("Tx execution failure: {:?}", tx);
                        }
                        _ => {
                            warn!("Tx execution unexpected result: {:?}", tx);
                        }
                    };
                    debug!("Tx receipt {:?}.", outcome.receipt);
                    // if has output, print it, useful when debugging
                    if !outcome.output.is_empty() {
                        debug!("Tx {:?} output {:?}.", tx, outcome.output);
                    }
                    debug!("Tx trace {:?}.", outcome.trace);
                    if let Some(vm_trace) = &outcome.vm_trace {
                        debug!("Tx vm_trace length (number of operations) {}.", vm_trace.operations.len());
                    }
                    debug!("Tx vm trace {:?}.", outcome.vm_trace);
                }
                // after applying transactions, commit
                statedb.commit().unwrap();
            }
        });
    }
}

fn update_transaction_sequence(
    blockdb: &BlockDatabase,
    chain: &BlockChain,
    sender: &channel::Sender<Vec<Transaction>>,
) {
    let diff = chain.update_ledger().unwrap();
    PERFORMANCE_COUNTER.record_deconfirm_transaction_blocks(diff.1.len());

    for hash in diff.0 {
        let block = blockdb.get(&hash).unwrap().unwrap();
        if block.header.extra_content == LAZY_ANNOTATION {
            continue;
        }
        PERFORMANCE_COUNTER.record_confirm_transaction_block(&block);
        let content = match block.content {
            Content::Transaction(data) => data,
            _ => unreachable!(),
        };
        sender.send(content.transactions).unwrap();
    }
    for _hash in diff.1 {
        warn!("Deconfim (Remove) tx shouldn't happen.");
    }
}
