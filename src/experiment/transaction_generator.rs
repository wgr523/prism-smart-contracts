use crate::experiment::performance_counter::PERFORMANCE_COUNTER;
use crate::handler::new_transaction;
use crate::miner::memory_pool::MemoryPool;
use crate::network::server::Handle as ServerHandle;

use crate::wallet::Wallet;
use crossbeam::channel;
use log::{info, debug, warn};
use rand::Rng;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time;
use ethereum_types::Address;
use std::collections::HashSet;
use rand::distributions::WeightedIndex;
use rand::distributions::Distribution;
use rand::seq::SliceRandom;

pub enum ControlSignal {
    Start(u64),
    Step(u64),
    Stop,
    SetArrivalDistribution(ArrivalDistribution),
    SetValueDistribution(ValueDistribution),
    SetTransactionType(String),
}

pub enum ArrivalDistribution {
    Uniform(UniformArrival),
}

pub struct UniformArrival {
    pub interval: u64, // ms
}

pub enum ValueDistribution {
    Uniform(UniformValue),
}

pub struct UniformValue {
    pub min: u64,
    pub max: u64,
}

enum State {
    Continuous(u64),
    Paused,
    Step(u64),
}

pub struct TransactionGenerator {
    wallet: Arc<Wallet>,
    server: ServerHandle,
    mempool: Arc<Mutex<MemoryPool>>,
    control_chan: channel::Receiver<ControlSignal>,
    arrival_distribution: ArrivalDistribution,
    value_distribution: ValueDistribution,
    state: State,
    transaction_type: String,
}

impl TransactionGenerator {
    pub fn new(
        wallet: &Arc<Wallet>,
        server: &ServerHandle,
        mempool: &Arc<Mutex<MemoryPool>>,
    ) -> (Self, channel::Sender<ControlSignal>) {
        let (tx, rx) = channel::unbounded();
        let instance = Self {
            wallet: Arc::clone(wallet),
            server: server.clone(),
            mempool: Arc::clone(mempool),
            control_chan: rx,
            arrival_distribution: ArrivalDistribution::Uniform(UniformArrival { interval: 100 }),
            value_distribution: ValueDistribution::Uniform(UniformValue { min: 50, max: 100 }),
            state: State::Paused,
            transaction_type: String::from("donothing"),
        };
        (instance, tx)
    }

    fn handle_control_signal(&mut self, signal: ControlSignal) {
        match signal {
            ControlSignal::Start(t) => {
                self.state = State::Continuous(t);
                info!("Transaction generator started");
            }
            ControlSignal::Stop => {
                self.state = State::Paused;
                info!("Transaction generator paused");
            }
            ControlSignal::Step(num) => {
                self.state = State::Step(num);
                info!(
                    "Transaction generator started to generate {} transactions",
                    num
                );
            }
            ControlSignal::SetArrivalDistribution(new) => {
                self.arrival_distribution = new;
            }
            ControlSignal::SetValueDistribution(new) => {
                self.value_distribution = new;
            }
            ControlSignal::SetTransactionType(t) => {
                self.transaction_type = t.clone();
                info!(
                    "Transaction generator started to generate {} type",
                    t
                );

            }
        }
    }

    pub fn start(mut self) {
        macro_rules! include_int_vec {
            ($file: expr) => {{
                let my_str = include_str!($file);
                my_str.lines().map(|x|x.parse::<u64>().expect("experiment/*.txt format error.")).collect()
            }};
        }

        thread::spawn(move || {
            let mut rng = rand::thread_rng();
            let addrs = self.wallet.addresses().unwrap();
            info!("Wallet controls {} key pairs.", addrs.len());
            let probs: Vec<u64> = include_int_vec!("from_address_counter.txt");
            let probs: Vec<u64> = probs.into_iter().take(addrs.len()).collect();
            let from_dist = WeightedIndex::new(&probs).unwrap();
            let probs: Vec<u64> = include_int_vec!("to_address_counter.txt");
            let mut probs: Vec<u64> = probs.into_iter().take(addrs.len()).collect();
            probs.shuffle(&mut rng);
            let to_dist = WeightedIndex::new(&probs).unwrap();
            //let mut addr_iter = addrs.iter().cycle();
            let mut has_erc20_ico: HashSet<Address> = HashSet::new();
            loop {
                let tx_gen_start = time::Instant::now();
                // check the current state and try to receive control message
                match self.state {
                    State::Continuous(_) | State::Step(_) => match self.control_chan.try_recv() {
                        Ok(signal) => {
                            self.handle_control_signal(signal);
                            continue;
                        }
                        Err(channel::TryRecvError::Empty) => {}
                        Err(channel::TryRecvError::Disconnected) => {
                            panic!("Transaction generator control channel detached")
                        }
                    },
                    State::Paused => {
                        // block until we get a signal
                        let signal = self.control_chan.recv().unwrap();
                        self.handle_control_signal(signal);
                        continue;
                    }
                }
                // check whether the mempool is already full
                if let State::Continuous(throttle) = self.state {
                    if self.mempool.lock().unwrap().len() as u64 >= throttle {
                        // if the mempool is full, just skip this transaction
                        let interval: u64 = match &self.arrival_distribution {
                            ArrivalDistribution::Uniform(d) => d.interval,
                        };
                        let interval = time::Duration::from_micros(interval);
                        thread::sleep(interval);
                        continue;
                    }
                }
                let value: u64 = match &self.value_distribution {
                    ValueDistribution::Uniform(d) => {
                        if d.min == d.max {
                            d.min
                        } else {
                            rng.gen_range(d.min, d.max)
                        }
                    }
                };
                //let addr = addr_iter.next().expect("Failed to get a key pair from wallet");
                // the from (sender) addr
                let addr = addrs.get(from_dist.sample(&mut rng)).unwrap();
                let to_addr = addrs.get(to_dist.sample(&mut rng)).unwrap();
                let transaction = match self.transaction_type.as_ref() {
                    "payment" => self.wallet.create_transaction_payment(addr, to_addr, value.into()),
                    "donothing" => self.wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xf01), hex::decode("448f30a3").unwrap()),
                    "cpuheavy" => self.wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xf02), hex::decode("fe91386500000000000000000000000000000000000000000000000000000000000000ff").unwrap()),
                    "ioheavy" => self.wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xf03), hex::decode("b6a3f0e900000000000000000000000000000000000000000000000000000000000000ff").unwrap()),
                    "simpleballot" => {
                        panic!("In current experiment we don't want to call simple ballot");
                        // need to append a random 0 or 1 to it
                        let mut data = hex::decode("0121b93f00000000000000000000000000000000000000000000000000000000000000").unwrap();
                        // append a random value (represents to whom we vote)
                        data.append(&mut vec![rng.gen_range(0u8, 2u8)]);
                        self.wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xe00), data)
                    }
                    "size600" => self.wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xf01), hex::decode("448f30a30000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap()),
                    "cryptokitties" => {
                        let mut data = hex::decode("0d9f5aed").unwrap();
                        // generate 2 random u256 parameter (= 4 u128)
                        for _ in 0..4 {
                            let t: u128 = rng.gen();
                            data.extend_from_slice(t.to_be_bytes().as_ref());
                        }
                        // last parameter, target block number is 0
                        data.append(&mut hex::decode("0000000000000000000000000000000000000000000000000000000000000000").unwrap());
                        self.wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xe01), data)
                    }
                    "simpleerc20" => {
                        if has_erc20_ico.contains(addr) {
                            // transfer tokens to self
                            let mut data = hex::decode("a9059cbb000000000000000000000000").unwrap();
                            data.extend_from_slice(to_addr.as_bytes());
                            data.append(&mut hex::decode("0000000000000000000000000000000000000000000000000000000000000001").unwrap());
                            self.wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xe02), data)
                        } else {
                            has_erc20_ico.insert(addr.clone());
                            // mint tokens, number is very large: 0xffffffffffffffff
                            self.wallet.create_transaction_call(addr, &Address::from_low_u64_be(0xe02), hex::decode("a0712d68000000000000000000000000000000000000000000000000ffffffffffffffff").unwrap())
                        }
                    }
                    _ => panic!("Transaction type does not match any known one."),
                };
                PERFORMANCE_COUNTER.record_generate_transaction(&transaction);
                match transaction {
                    Ok(t) => {
                        new_transaction(t, &self.mempool, &self.server);
                        // if we are in stepping mode, decrease the step count
                        if let State::Step(step_count) = self.state {
                            if step_count - 1 == 0 {
                                self.state = State::Paused;
                            } else {
                                self.state = State::Step(step_count - 1);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to generate transaction: {}", e);
                    }
                };
                let interval: u64 = match &self.arrival_distribution {
                    ArrivalDistribution::Uniform(d) => d.interval,
                };
                let interval = time::Duration::from_micros(interval);
                let time_spent = time::Instant::now().duration_since(tx_gen_start);
                let interval = {
                    if interval > time_spent {
                        interval - time_spent
                    } else {
                        time::Duration::new(0, 0)
                    }
                };
                thread::sleep(interval);
            }
        });
        info!("Transaction generator initialized into paused mode");
    }
}
