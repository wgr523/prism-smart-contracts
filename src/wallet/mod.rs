use ethereum_types::{U256};
use crate::transaction::*;
use crate::crypto::hash::{H256, Address};

use bincode::serialize;
use rand::rngs::OsRng;

use std::cell::RefCell;
use std::collections::HashMap;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::{error, fmt};
use hex::decode;
use keccak_hash::keccak;
use parity_crypto::publickey::{KeyPair, Random, Secret, Public, Generator};
use parity_bytes::Bytes;

pub const KEYPAIR_CF: &str = "KEYPAIR"; // &Address to &Secret

pub type Result<T> = std::result::Result<T, WalletError>;

/// A data structure to maintain key pairs and their coins, and to generate transactions.
pub struct Wallet {
    /// The underlying RocksDB handle.
    db: rocksdb::DB,
    /// The nonce,balance pair for each address TODO put into db
    nonce_balances: Mutex<HashMap<Address, (U256, U256)>>,
    /// Keep key pair (in pkcs8 bytes) in memory for performance, it's duplicated in database as well.
    keypairs: Mutex<HashMap<Address, KeyPair>>,
    counter: AtomicUsize,
}

#[derive(Debug)]
pub enum WalletError {
    InsufficientBalance,
    MissingKeyPair,
    DBError(rocksdb::Error),
}

impl fmt::Display for WalletError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            WalletError::InsufficientBalance => write!(f, "insufficient balance"),
            WalletError::MissingKeyPair => write!(f, "missing key pair for the requested address"),
            WalletError::DBError(ref e) => e.fmt(f),
        }
    }
}

impl error::Error for WalletError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            WalletError::DBError(ref e) => Some(e),
            _ => None,
        }
    }
}

impl From<rocksdb::Error> for WalletError {
    fn from(err: rocksdb::Error) -> WalletError {
        WalletError::DBError(err)
    }
}

impl Wallet {
    fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let keypair_cf =
            rocksdb::ColumnFamilyDescriptor::new(KEYPAIR_CF, rocksdb::Options::default());
        let mut db_opts = rocksdb::Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        let handle = rocksdb::DB::open_cf_descriptors(&db_opts, path, vec![keypair_cf])?;
        Ok(Self {
            db: handle,
            nonce_balances: Mutex::new(HashMap::new()),
            keypairs: Mutex::new(HashMap::new()),
            counter: AtomicUsize::new(0),
        })
    }

    pub fn new<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        rocksdb::DB::destroy(&rocksdb::Options::default(), &path)?;
        Self::open(path)
    }

    pub fn number_of_coins(&self) -> usize {
        self.counter.load(Ordering::Relaxed)
    }

    /// Generate a new key pair
    pub fn generate_keypair(&self) -> Result<Address> {
        let keypair: KeyPair = Random.generate().unwrap();
        self.load_keypair(keypair)
    }


    pub fn load_keypair(&self, keypair: KeyPair) -> Result<Address> {
        let addr: Address = keypair.address();

        let cf = self.db.cf_handle(KEYPAIR_CF).unwrap();
        self.db.put_cf(cf, addr.as_bytes(), (*(keypair.secret())).as_bytes())?;

        let mut keypairs = self.keypairs.lock().unwrap();
        keypairs.insert(addr, keypair);
        drop(keypairs);
        let mut nonce_balances = self.nonce_balances.lock().unwrap();
        nonce_balances.insert(addr, (0.into(), 0.into()));
        drop(nonce_balances);
        // nonce balance is already in state db, so no need to store in wallet db
        Ok(addr)
    }


    /// Get the list of addresses for which we have a key pair
    pub fn addresses(&self) -> Result<Vec<Address>> {
        let keypairs = self.keypairs.lock().unwrap();
        let addrs = keypairs.keys().cloned().collect();
        Ok(addrs)
    }

    fn contains_keypair(&self, addr: &Address) -> bool {
        let keypairs = self.keypairs.lock().unwrap();
        if keypairs.contains_key(addr) {
            return true;
        }
        false
    }

    /// Returns the sum of values of all the coin in the wallet
    pub fn balance(&self) -> Result<u64> {
        let nonce_balances = self.nonce_balances.lock().unwrap();
        let mut sum = 0u64;
        for (_k,(_nonce,balance)) in nonce_balances.iter() {
            sum += balance.as_u64();
        }
        Ok(sum)
    }

    /// Create a payment transaction
    pub fn create_transaction_payment(&self, sender_addr: &Address, receiver_addr: &Address, value: U256) -> Result<Transaction> {
        let keypairs = self.keypairs.lock().unwrap();
        let keypair = match keypairs.get(sender_addr) {
            Some(kp) => kp.clone(),
            None => return Err(WalletError::MissingKeyPair),
        };
        drop(keypairs);

        let mut nonce_balances = self.nonce_balances.lock().unwrap();
        let nonce_balance = nonce_balances.entry(keypair.address()).or_insert((0.into(), 0.into()));
        let nonce = nonce_balance.0;
        nonce_balance.0 = nonce_balance.0 + 1;
        drop(nonce_balances);

        let receiver_addr = receiver_addr.clone();
        let tx: Transaction = RawTransaction {
            nonce: nonce.into(),
            gas_price: 0.into(),
            gas: 10_000_000.into(),
            action: Action::Call(receiver_addr),
            value: value,
            data: vec![],
        }.sign(keypair.secret(), None);
        Ok(tx)
    }

    /// Create a payment transaction from and to the same addr
    pub fn create_transaction_self_payment(&self, addr: &Address, value: U256) -> Result<Transaction> {
        let keypairs = self.keypairs.lock().unwrap();
        let keypair = match keypairs.get(addr) {
            Some(kp) => kp.clone(),
            None => return Err(WalletError::MissingKeyPair),
        };
        drop(keypairs);

        let mut nonce_balances = self.nonce_balances.lock().unwrap();
        let nonce_balance = nonce_balances.entry(keypair.address()).or_insert((0.into(), 0.into()));
        let nonce = nonce_balance.0;
        nonce_balance.0 = nonce_balance.0 + 1;
        drop(nonce_balances);

        let receiver_addr = addr.clone();
        let tx: Transaction = RawTransaction {
            nonce: nonce.into(),
            gas_price: 0.into(),
            gas: 10_000_000.into(),
            action: Action::Call(receiver_addr),
            value: value,
            data: vec![],
        }.sign(keypair.secret(), None);
        Ok(tx)
    }

    /// Create a call transaction with our first keypair and 0 value
    pub fn create_transaction_call(&self, addr: &Address, receiver_addr: &Address, data: Bytes) -> Result<Transaction> {
        let keypairs = self.keypairs.lock().unwrap();
        let keypair = match keypairs.get(addr) {
            Some(kp) => kp.clone(),
            None => return Err(WalletError::MissingKeyPair),
        };
        drop(keypairs);

        let mut nonce_balances = self.nonce_balances.lock().unwrap();
        let nonce_balance = nonce_balances.entry(keypair.address()).or_insert((0.into(), 0.into()));
        let nonce = nonce_balance.0;
        nonce_balance.0 = nonce_balance.0 + 1;
        drop(nonce_balances);

        let tx: Transaction = RawTransaction {
            nonce: nonce.into(),
            gas_price: 0.into(),
            gas: 10_000_000.into(),
            action: Action::Call(receiver_addr.clone()),
            value: 0.into(),
            data,
        }.sign(keypair.secret(), None).into();
        Ok(tx)
    }

    /// Create a create transaction with our first keypair and 0 value
    pub fn create_transaction_create(&self, data: Bytes) -> Result<Transaction> {
        let keypairs = self.keypairs.lock().unwrap();
        let keypair = match keypairs.values().next() {
            Some(kp) => kp.clone(),
            None => return Err(WalletError::MissingKeyPair),
        };
        drop(keypairs);

        let mut nonce_balances = self.nonce_balances.lock().unwrap();
        let nonce_balance = nonce_balances.entry(keypair.address()).or_insert((0.into(), 0.into()));
        let nonce = nonce_balance.0;
        nonce_balance.0 = nonce_balance.0 + 1;
        drop(nonce_balances);

        let tx: Transaction = RawTransaction {
            nonce: nonce.into(),
            gas_price: 0.into(),
            gas: 10_000_000.into(),
            action: Action::Create,
            value: 0.into(),
            data,
        }.sign(keypair.secret(), None).into();
        Ok(tx)
    }
}

#[cfg(test)]
pub mod tests {}
