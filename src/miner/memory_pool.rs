use crate::crypto::hash::{Hashable, H256};
use crate::transaction::Transaction;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::VecDeque;

/// transactions storage
#[derive(Debug)]
pub struct MemoryPool {
    /// Number of transactions
    num_transactions: u64,
    /// Maximum number that the memory pool can hold
    max_transactions: u64,
    /// Counter for storage index
    counter: u64,
    /// By-hash storage
    by_hash: HashMap<H256, Entry>,
    /// Storage for order by storage index, it is equivalent to FIFO
    by_storage_index: BTreeMap<u64, H256>,
}

#[derive(Debug, Clone)]
pub struct Entry {
    /// Transaction
    pub transaction: Transaction,
    /// counter of the tx
    storage_index: u64,
}

impl MemoryPool {
    pub fn new(size_limit: u64) -> Self {
        Self {
            num_transactions: 0,
            max_transactions: size_limit,
            counter: 0,
            by_hash: HashMap::new(),
            by_storage_index: BTreeMap::new(),
        }
    }

    /// Insert a tx into memory pool.
    pub fn insert(&mut self, tx: Transaction) {
        if self.num_transactions > self.max_transactions {
            return;
        }
        // assumes no duplicates nor double spends
        let hash = <Transaction as Hashable>::hash(&tx);
        let entry = Entry {
            transaction: tx,
            storage_index: self.counter,
        };
        self.counter += 1;

        // add to btree
        self.by_storage_index.insert(entry.storage_index, hash);

        // add to hashmap
        self.by_hash.insert(hash, entry);

        self.num_transactions += 1;
    }

    pub fn get(&self, h: &H256) -> Option<&Entry> {
        let entry = self.by_hash.get(h)?;
        Some(entry)
    }

    /// Check whether a tx hash is in memory pool
    /// When adding tx into mempool, should check this.
    pub fn contains(&self, h: &H256) -> bool {
        self.by_hash.contains_key(h)
    }

    fn remove_and_get(&mut self, hash: &H256) -> Option<Entry> {
        let entry = self.by_hash.remove(hash)?;
        self.by_storage_index.remove(&entry.storage_index);
        self.num_transactions -= 1;
        Some(entry)
    }

    /// Remove a tx by its hash
    pub fn remove_by_hash(&mut self, hash: &H256) {
        self.remove_and_get(hash);
    }

    /// get n transaction by fifo
    pub fn get_transactions(&self, n: u32) -> Vec<Transaction> {
        self.by_storage_index
            .values()
            .take(n as usize)
            .map(|hash| self.get(hash).unwrap().transaction.clone())
            .collect()
    }

    /// get size/length
    pub fn len(&self) -> usize {
        self.by_hash.len()
    }
}

#[cfg(test)]
pub mod tests {}
