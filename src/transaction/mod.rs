use crate::experiment::performance_counter::PayloadSize;
pub use common_types::transaction::{Transaction as RawTransaction, SignedTransaction as Transaction, *};
use crate::crypto::hash::{Hashable, H256};

impl PayloadSize for Transaction {
    /// Return the size in bytes
    fn size(&self) -> usize {
        // TODO: Caculate real size for Parity-Ethereum transaction
        1
    }
}

impl Hashable for Transaction {
    fn hash(&self) -> H256 {
        self.hash().into()
    }
}

#[cfg(any(test))]
pub mod tests {}
