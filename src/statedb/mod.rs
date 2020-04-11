use crate::crypto::hash::{Address, EthereumH256 as H256};
use crate::transaction::{Action, Transaction, RawTransaction};
use std::sync::{Arc, Mutex};

use kvdb::DBTransaction;
use ethereum_types::U256;
use executive_state::ExecutiveState;
use parity_bytes::Bytes;
use parity_crypto::publickey::{KeyPair, Random, Secret, Public, Generator};

pub type Result<T> = std::result::Result<T, StateDatabaseError>;

pub struct StateDatabase {
    // State has un-synchrony fields such as HashMap, thus wrap it with mutex
    state: Mutex<account_state::state::State<state_db::StateDB>>,
    // Machine: the parameters to apply transaction, should be constant during the run
    machine: machine::Machine,
}

impl std::fmt::Debug for StateDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let state = self.state.lock().unwrap();
        write!(f, "{:?}", state)
    }
}

#[derive(Debug)]
pub enum StateDatabaseError {
    IOError(std::io::Error),
    EthcoreError(common_types::errors::EthcoreError),
    TrieError(Box<patricia_trie_ethereum::TrieError>),
    Trace(String,String),
}

impl std::fmt::Display for StateDatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            StateDatabaseError::IOError(ref e) => e.fmt(f),
            StateDatabaseError::EthcoreError(ref e) => e.fmt(f),
            StateDatabaseError::TrieError(ref e) => e.fmt(f),
            StateDatabaseError::Trace(ref e, ref t) => write!(f, "Trace: {}\n\nVMTrace: {}", e,t),
        }
    }
}

impl std::error::Error for StateDatabaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            StateDatabaseError::IOError(ref e) => Some(e),
            StateDatabaseError::EthcoreError(ref e) => Some(e),
            StateDatabaseError::TrieError(ref e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for StateDatabaseError {
    fn from(err: std::io::Error) -> StateDatabaseError {
        StateDatabaseError::IOError(err)
    }
}

impl From<common_types::errors::EthcoreError> for StateDatabaseError {
    fn from(err: common_types::errors::EthcoreError) -> StateDatabaseError {
        StateDatabaseError::EthcoreError(err)
    }
}

impl From<Box<patricia_trie_ethereum::TrieError>> for StateDatabaseError {
    fn from(err: Box<patricia_trie_ethereum::TrieError>) -> StateDatabaseError {
        StateDatabaseError::TrieError(err)
    }
}

impl StateDatabase {
    /// Open the database at the given path, and create a new one if one is missing.
    pub fn open<P: AsRef<std::path::Path>>(path: P, root: Option<H256>) -> Result<Self> {
        // use this spec and the corresponding machine since it looks fine, may create our spec in the future
        let mut spec = spec::new_prism_test();
        let machine = spec::new_prism_test_machine();
        if let Some(root) = root {
            spec.state_root = root;
        }
        // default factories will do
	let factories = trie_vm_factories::Factories::default();
        let client_config = ethcore::client::ClientConfig::default();

        let restoration_db_handler = parity_ethereum::db::restoration_db_handler(path.as_ref(), &client_config);
        let app_db = restoration_db_handler.open(path.as_ref())?;
        // A journal db using app_db's kvdb as backing and column family is 0
	let journal_db = journaldb::new(Arc::clone(app_db.key_value()), client_config.pruning, 0);
        // state_db created by new() has parent_hash==None, so it doesn't use local (dirty) cache
	let mut state_db = state_db::StateDB::new(journal_db, client_config.state_cache_size);
        if state_db.journal_db().is_empty() {
            // Sets the correct state root.
            state_db = spec.ensure_db_good(state_db, &factories)?;
            let mut batch = DBTransaction::new();
            state_db.journal_under(&mut batch, 0, &spec.genesis_header().hash())?;
            state_db.journal_db().backing().write(batch)?;
        }
        let state = account_state::state::State::from_existing(
            state_db,
            spec.state_root,
            spec.engine.account_start_nonce(0),
            factories,
        )?;
        let state = Mutex::new(state);
        Ok(Self { state, machine })
    }

    /// Create a new database at the given path, and initialize the content.
    pub fn new<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        //kvdb rocksdb don't have destroy??? so we use std::fs::remove_dir_all
        if path.as_ref().is_dir() {
            std::fs::remove_dir_all(&path)?;
        }
        let db = Self::open(&path, None)?;

        Ok(db)
    }

    pub fn root(&self) -> H256 {
        let state = self.state.lock().unwrap();
        state.root().clone()
    }


    /// Increase the balance in state, use NoEmpty mode.
    pub fn add_balance(&self, a: &Address, incr: &U256) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.add_balance(a,incr,account_state::CleanupMode::NoEmpty)?;
        Ok(())
    }

    pub fn balance(&self, a: &Address) -> Result<U256> {
        let state = self.state.lock().unwrap();
        Ok(state.balance(a)?)
    }

    pub fn nonce(&self, a: &Address) -> Result<U256> {
        let state = self.state.lock().unwrap();
        Ok(state.nonce(a)?)
    }

    pub fn code(&self, a: &Address) -> Result<Option<Arc<Bytes>>> {
        let state = self.state.lock().unwrap();
        Ok(state.code(a)?)
    }

    pub fn storage_at(&self, a: &Address, key: &H256) -> Result<H256> {
        let state = self.state.lock().unwrap();
        Ok(state.storage_at(a,key)?)
    }

    pub fn machine(&self) -> &machine::Machine {
        &self.machine
    }

    pub fn apply(&self, t: &Transaction) -> Result<executive_state::ApplyOutcome<trace::FlatTrace, trace::VMTrace>> {
        self.apply_with_env_info(t, None)
    }

    fn apply_with_env_info(&self, t: &Transaction, env_info: Option<&vm::EnvInfo>) -> Result<executive_state::ApplyOutcome<trace::FlatTrace, trace::VMTrace>> {
        let mut default_info: vm::EnvInfo;
        let info = match env_info {
            Some(info) => info,
            None => {
                default_info = vm::EnvInfo::default();
                // we don't care about gas limit now, so set it to max
                default_info.gas_limit = U256::MAX;
                // info.gas_used = gas already used of this block (we don't care so set to 0)
                // set the block number to 1 in order to be compatible with cryptokitties
                default_info.number = 1;
                // give a default hash. hash doesn't affect the execution time of contracts
                default_info.last_hashes = Arc::new(vec![Default::default()]);
                // info.author = block miner, whom fees goes to
                &default_info
            }
        };
        let mut state = self.state.lock().unwrap();
        // set tracing=false
        let apply_outcome = state.apply(info, &self.machine, t, false)?;

        Ok(apply_outcome)
    }

    pub fn mem_used(&self) -> usize {
        let state = self.state.lock().unwrap();
        state.db().mem_used()
    }

    /// Commit changes from state's cache to backing db
    /// If need the up-to-date root, should call this
    pub fn commit(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        // commit changes (in cache) to backing journaldb
        state.commit()?;
        // safe to clear cache since we have committed
        state.clear();
        let db = state.db_mut();
        let mut batch = DBTransaction::new();
        db.journal_under(&mut batch, 0, &Default::default())?;
        db.journal_db().backing().write(batch)?;
        /* sync_cache seems to have no effect here because db.parent_hash==None and there is no
         * dirty cache in statedb
        db.sync_cache(&[], &[], true);*/
        Ok(())
    }

    // used for test
    // test whether code is a proper init code
    #[cfg(test_utility)]
    pub fn create_contract(&self, code: Bytes) -> Result<Address>{
        let mut state = self.state.lock().unwrap();
        let keypair: KeyPair = Random.generate().unwrap();
	let addr = keypair.address();
        let value: U256 = 1000000.into();
        state.add_balance(&addr, &value, account_state::CleanupMode::NoEmpty)?;
        let t = RawTransaction {
            nonce: 0.into(),
            gas_price: 0.into(),
            gas: U256::MAX,
            action: Action::Create,
            value: 0.into(),
            data: code,
        }.sign(&keypair.secret(), None);
        let mut default_info = vm::EnvInfo::default();
        // we don't care about gas limit now, so set it to max
        default_info.gas_limit = U256::MAX;
        let outcome = state.apply(&default_info, &self.machine, &t, true)?;
        let contract_addr = match &outcome.trace[0].result {
            trace::trace::Res::Create(res) => res.address.clone(),
            _ => {
                return Err(StateDatabaseError::Trace(
                        format!("{:?}", outcome.trace),
                        format!("{:?}", outcome.vm_trace)
                        ));
            }
        };
        Ok(contract_addr)
    }

}

// for test
#[cfg(test_utility)]
pub fn get_temp_state_database() -> StateDatabase {
    let state = ethcore::test_helpers::get_temp_state();
    let state = Mutex::new(state);
    let machine = spec::new_prism_test_machine();
    StateDatabase {
        state,
        machine,
    }
}

#[cfg(test)]
mod test {
    use super::StateDatabase;
    use crate::crypto::hash::{Address, EthereumH256 as H256};
    use crate::transaction::{Action, RawTransaction};
    use ethereum_types::U256;
    use parity_crypto::publickey::{KeyPair, Random, Secret, Public, Generator};
    use std::sync::Mutex;

    fn get_temp_state_database() -> StateDatabase {
        let state = ethcore::test_helpers::get_temp_state();
        let state = Mutex::new(state);
        let machine = spec::new_prism_test_machine();
        StateDatabase {
            state,
            machine,
        }
    }

    #[test]
    fn balance() {
        let statedb = get_temp_state_database();
	let addr = Address::repeat_byte(8u8);
        let value: U256 = 19.into();
        statedb.add_balance(&addr, &value).unwrap();
        assert_eq!(statedb.balance(&addr).unwrap(), value);
    }

    #[test]
    fn apply_payment() {
        let statedb = get_temp_state_database();
        let keypair: KeyPair = Random.generate().unwrap();
	let addr = keypair.address();
        let value: U256 = 12889.into();
        statedb.add_balance(&addr, &value).unwrap();
        let receiver_addr = Address::from_low_u64_be(0xa);
        let t = RawTransaction {
            nonce: 0.into(),
            gas_price: 0.into(),
            gas: 100_000.into(),
            action: Action::Call(receiver_addr.clone()),
            value: 100.into(),
            data: vec![],
        }.sign(&keypair.secret(), None);
        let outcome = statedb.apply(&t).unwrap();
        assert_eq!(statedb.balance(&addr).unwrap(), value-U256::from(100));
        assert_eq!(statedb.nonce(&addr).unwrap(), 1.into());
        assert_eq!(statedb.balance(&receiver_addr).unwrap(), 100.into());
        let t = RawTransaction {
            nonce: 1.into(),
            gas_price: 0.into(),
            gas: 100_000.into(),
            action: Action::Call(receiver_addr.clone()),
            value: 100.into(),
            data: vec![],
        }.sign(&keypair.secret(), None);
        let outcome = statedb.apply(&t).unwrap();
        assert_eq!(statedb.balance(&addr).unwrap(), value-U256::from(100*2));
        assert_eq!(statedb.nonce(&addr).unwrap(), 2.into());
        assert_eq!(statedb.balance(&receiver_addr).unwrap(), (100*2).into());
    }

    #[test]
    fn apply_create_simple_contract() {
        let statedb = get_temp_state_database();
        let keypair: KeyPair = Random.generate().unwrap();
	let addr = keypair.address();
        let value: U256 = 12889.into();
        statedb.add_balance(&addr, &value).unwrap();
        /* this code function is to get the sender's address and store at 0x00
         * CALLER
         * BALANCE
         * PUSH1 0
         * SSTORE (arguments: 0, balance): store balance at position 0x00
         */
        let code = hex::decode("3331600055").unwrap();
        /* code is learnt from parity-ethereum/ethcore/machine/src/executive.rs test_create_contract()
         *
         * PUSH1 code.len
         * DUPLICATE (code.len)
         * PUSH1 12 (offset that the code starts
         * PUSH1 0 (offset in memory to store the code)
         * CODECOPY (arguments: 0,12,code.len): copy code from 12 of length code.len to memory
         * starting at 0
         * PUSH1 0
         * RETURN (arguments: 0,code.len): return memory content between 0 and code.len
         */
        let mut init_code: Vec<u8> = vec![96,code.len() as u8,128,96,12,96,0,57,96,0,243,0];
        init_code.extend(code.clone());
        let t = RawTransaction {
            nonce: 0.into(),
            gas_price: 0.into(),
            gas: 100_000.into(),
            action: Action::Create,
            value: 100.into(),
            data: init_code.clone(),
        }.sign(&keypair.secret(), None);
        let outcome = statedb.apply(&t).unwrap();
        // show the apply outcome
        println!("{:?}", outcome.receipt);
        println!("{:?}", outcome.output);
        println!("{:?}", outcome.trace);
        println!("{:?}", outcome.vm_trace);
        // get contract's address from trace
        let receiver_addr = match &outcome.trace[0].result {
            trace::trace::Res::Create(res) => res.address.clone(),
            _ => {
                unreachable!()
            }
        };
        assert_eq!(statedb.balance(&addr).unwrap(), value-U256::from(100));
        assert_eq!(statedb.nonce(&addr).unwrap(), 1.into());
        assert_eq!(statedb.balance(&receiver_addr).unwrap(), (100).into());
        assert_eq!(code, *statedb.code(&receiver_addr).unwrap().unwrap());

        // call this contract
        let t = RawTransaction {
            nonce: 1.into(),
            gas_price: 0.into(),
            gas: 100_000.into(),
            action: Action::Call(receiver_addr),
            value: 100.into(),
            data: vec![0u8],//TODO don't know if pass 0 is okay?
        }.sign(&keypair.secret(), None);
        let outcome = statedb.apply(&t).unwrap();
        // show the apply outcome
        println!("{:?}", outcome.receipt);
        println!("{:?}", outcome.output);
        println!("{:?}", outcome.trace);
        println!("{:?}", outcome.vm_trace);

        // get the storage
        let b = statedb.storage_at(&receiver_addr, &H256::from_low_u64_be(0)).unwrap();
        let b: U256 = b.to_fixed_bytes().into();
        assert_eq!(b, statedb.balance(&addr).unwrap());
        assert_eq!(statedb.balance(&addr).unwrap(), value-U256::from(100*2));
        assert_eq!(statedb.nonce(&addr).unwrap(), 2.into());
        assert_eq!(statedb.balance(&receiver_addr).unwrap(), (100*2).into());
    }

    #[test]
    #[cfg(test_utility)]
    fn apply_create_contract() {
        let statedb = get_temp_state_database();
        /* this code function is to get the sender's address and store at 0x00
         * CALLER
         * BALANCE
         * PUSH1 0
         * SSTORE (arguments: 0, balance): store balance at position 0x00
         */
        let code = hex::decode("3331600055").unwrap();
        /* code is learnt from parity-ethereum/ethcore/machine/src/executive.rs test_create_contract()
         *
         * PUSH1 code.len
         * DUPLICATE (code.len)
         * PUSH1 12 (offset that the code starts
         * PUSH1 0 (offset in memory to store the code)
         * CODECOPY (arguments: 0,12,code.len): copy code from 12 of length code.len to memory
         * starting at 0
         * PUSH1 0
         * RETURN (arguments: 0,code.len): return memory content between 0 and code.len
         */
        let mut init_code: Vec<u8> = vec![96,code.len() as u8,128,96,12,96,0,57,96,0,243,0];
        init_code.extend(code.clone());
        let contract_addr = statedb.create_contract(init_code).unwrap();
    }
}
