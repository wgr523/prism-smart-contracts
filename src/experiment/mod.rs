pub mod performance_counter;
pub mod transaction_generator;

use crate::crypto::hash::Address;
use crate::statedb::StateDatabase;
use crate::wallet::Wallet;
use bincode::serialize;
use std::sync::{Arc, Mutex};
use std::thread;
use ethereum_types::U256;

pub fn ico(
    recipients: &[Address], // addresses of all the ico recipients
    statedb: &Arc<StateDatabase>,
    wallet: &Arc<Wallet>,
    num_coins: usize,
) -> Result<(), rocksdb::Error> {
    let num_coins: U256 = num_coins.into();
    for recipient in recipients.iter() {
        statedb.add_balance(recipient, &num_coins).expect("ICO state db error");
    }

    statedb.commit().expect("ICO state db error");
    Ok(())
}
