use crate::blockchain::BlockChain;
use crate::experiment::performance_counter::PERFORMANCE_COUNTER;
use crate::experiment::transaction_generator;
use crate::miner::memory_pool::MemoryPool;
use crate::miner::Handle as MinerHandle;
use crate::network::server::Handle as ServerHandle;
use crate::statedb::StateDatabase;
use crate::wallet::Wallet;
use crate::handler::new_transaction;
use crate::crypto::hash::{Address, H256};

use log::info;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use tiny_http::Header;
use tiny_http::Response;
use tiny_http::Server as HTTPServer;
use url::Url;

pub struct Server {
    transaction_generator_handle: crossbeam::Sender<transaction_generator::ControlSignal>,
    handle: HTTPServer,
    miner: MinerHandle,
    statedb: Arc<StateDatabase>,
    wallet: Arc<Wallet>,
    blockchain: Arc<BlockChain>,
    mempool: Arc<Mutex<MemoryPool>>,
    /// the network server handle
    server: ServerHandle,
}

#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

#[derive(Serialize)]
struct WalletBalanceResponse {
    balance: u64,
}

#[derive(Serialize)]
struct UtxoSnapshotResponse {
    checksum: String,
}

#[derive(Serialize)]
struct BlockchainSnapshotResponse {
    leaders: Vec<String>,
}

macro_rules! respond_result {
    ( $req:expr, $success:expr, $message:expr ) => {{
        let content_type = "Content-Type: application/json".parse::<Header>().unwrap();
        let payload = ApiResponse {
            success: $success,
            message: $message.to_string(),
        };
        let resp = Response::from_string(serde_json::to_string_pretty(&payload).unwrap())
            .with_header(content_type);
        $req.respond(resp).unwrap();
    }};
}

macro_rules! respond_json {
    ( $req:expr, $message:expr ) => {{
        let content_type = "Content-Type: application/json".parse::<Header>().unwrap();
        let resp = Response::from_string(serde_json::to_string_pretty(&$message).unwrap())
            .with_header(content_type);
        $req.respond(resp).unwrap();
    }};
}

impl Server {
    pub fn start(
        addr: std::net::SocketAddr,
        statedb: &Arc<StateDatabase>,
        wallet: &Arc<Wallet>,
        blockchain: &Arc<BlockChain>,
        server: &ServerHandle,
        miner: &MinerHandle,
        mempool: &Arc<Mutex<MemoryPool>>,
        txgen_control_chan: crossbeam::Sender<transaction_generator::ControlSignal>,
    ) {
        let handle = HTTPServer::http(&addr).unwrap();
        let server = Self {
            handle,
            transaction_generator_handle: txgen_control_chan,
            miner: miner.clone(),
            statedb: Arc::clone(statedb),
            wallet: Arc::clone(wallet),
            blockchain: Arc::clone(blockchain),
            mempool: Arc::clone(mempool),
            server: server.clone(),
        };
        thread::spawn(move || {
            for req in server.handle.incoming_requests() {
                let transaction_generator_handle = server.transaction_generator_handle.clone();
                let miner = server.miner.clone();
                let statedb = Arc::clone(&server.statedb);
                let wallet = Arc::clone(&server.wallet);
                let blockchain = Arc::clone(&server.blockchain);
                let mempool = Arc::clone(&server.mempool);
                let network_server = server.server.clone();
                thread::spawn(move || {
                    // a valid url requires a base
                    let base_url = Url::parse(&format!("http://{}/", &addr)).unwrap();
                    let url = match base_url.join(req.url()) {
                        Ok(u) => u,
                        Err(e) => {
                            respond_result!(req, false, format!("error parsing url: {}", e));
                            return;
                        }
                    };
                    match url.path() {
                        "/blockchain/snapshot" => {
                            let leaders = blockchain.proposer_leaders().unwrap();
                            let leader_hash_strings: Vec<String> =
                                leaders.iter().map(|x| x.to_string()).collect();
                            let resp = BlockchainSnapshotResponse {
                                leaders: leader_hash_strings,
                            };
                            respond_json!(req, resp);
                        }
                        "/utxo/snapshot" => {
                            let checksum = statedb.root();
                            let resp = UtxoSnapshotResponse {
                                checksum: base64::encode(&checksum),
                            };
                            respond_json!(req, resp);
                        }
                        "/wallet/balance" => {
                            let resp = WalletBalanceResponse {
                                balance: wallet.balance().unwrap(),
                            };
                            respond_json!(req, resp);
                        }
                        /*
                        "/wallet/pay" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let receiver_addr: Option<Address> = params
                                .get("address")
                                .and_then(|s| hex::decode(s).ok()
                                    .and_then(|v| if v.len() == Address::len_bytes() {
                                        Some(Address::from_slice(v.as_slice()))
                                    } else {
                                        None
                                    }));
                            let value = match params.get("value") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing value");
                                    return;
                                }
                            };
                            let value = match value.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing value: {}", e)
                                    );
                                    return;
                                }
                            };
                            match receiver_addr {
                                Some(receiver_addr) => {
                                    match wallet.create_transaction_payment(&receiver_addr, value.into()) {
                                        Ok(t) => {
                                            new_transaction(t, &mempool, &network_server);
                                            respond_result!(req, true, "ok");
                                        }
                                        Err(_) => respond_result!(req, false, "error of creating transaction"),

                                    };
                                }
                                None => respond_result!(req, false, "error of transaction, you need address and value"),
                            };
                        }
                        */
                        "/wallet/call" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let receiver_addr: Option<Address> = params
                                .get("address")
                                .and_then(|s| hex::decode(s).ok()
                                    .and_then(|v| if v.len() == Address::len_bytes() {
                                        Some(Address::from_slice(v.as_slice()))
                                    } else {
                                        None
                                    }));
                            let data: Option<Vec<u8>> = params
                                .get("data")
                                .and_then(|s| hex::decode(s).ok());
                            let data: Vec<u8> = data.unwrap_or(vec![]);
                            match receiver_addr {
                                Some(receiver_addr) => {
                                    let addrs = wallet.addresses().unwrap();
                                    let addr = match addrs.iter().next() {
                                        Some(a) => a,
                                        _ => {
                                            respond_result!(req, false, "error of creating transaction: failed to get a key pair from wallet");
                                            return;
                                        }
                                    };
                                    match wallet.create_transaction_call(addr, &receiver_addr, data) {
                                        Ok(t) => {
                                            new_transaction(t, &mempool, &network_server);
                                            respond_result!(req, true, "ok");
                                        }
                                        Err(_) => respond_result!(req, false, "error of creating transaction"),

                                    };
                                }
                                None => respond_result!(req, false, "error of call transaction, you need contract address and data"),
                            };
                        }
                        "/wallet/create" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let data: Option<Vec<u8>> = params
                                .get("data")
                                .and_then(|s| hex::decode(s).ok());
                            match data {
                                Some(data) => {
                                    match wallet.create_transaction_create(data) {
                                        Ok(t) => {
                                            new_transaction(t, &mempool, &network_server);
                                            respond_result!(req, true, "ok");
                                        }
                                        Err(_) => respond_result!(req, false, "error of creating transaction"),

                                    };
                                }
                                None => respond_result!(req, false, "error of creating transaction, you need data (the init code of a contract)"),
                            };
                        }
                        "/miner/start" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let lambda = match params.get("lambda") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing lambda");
                                    return;
                                }
                            };
                            let lambda = match lambda.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing lambda: {}", e)
                                    );
                                    return;
                                }
                            };
                            let lazy = match params.get("lazy") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing lazy switch");
                                    return;
                                }
                            };
                            let lazy = match lazy.parse::<bool>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing lazy switch: {}", e)
                                    );
                                    return;
                                }
                            };
                            let prob = match params.get("prob") {
                                Some(v) => v,
                                None => {
                                    "0.0"
                                }
                            };
                            let prob = match prob.parse::<f64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing lazy prob: {}", e)
                                    );
                                    return;
                                }
                            };
                            miner.start(lambda, lazy, prob);
                            respond_result!(req, true, "ok");
                        }
                        "/miner/step" => {
                            miner.step();
                            respond_result!(req, true, "ok");
                        }
                        "/telematics/snapshot" => {
                            respond_json!(req, PERFORMANCE_COUNTER.snapshot());
                        }
                        "/transaction-generator/start" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let throttle = match params.get("throttle") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing throttle");
                                    return;
                                }
                            };
                            let throttle = match throttle.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing throttle: {}", e)
                                    );
                                    return;
                                }
                            };
                            let control_signal =
                                transaction_generator::ControlSignal::Start(throttle);
                            match transaction_generator_handle.send(control_signal) {
                                Ok(()) => respond_result!(req, true, "ok"),
                                Err(e) => respond_result!(
                                    req,
                                    false,
                                    format!(
                                        "error sending control signal to transaction generator: {}",
                                        e
                                    )
                                ),
                            }
                        }
                        "/transaction-generator/stop" => {
                            let control_signal = transaction_generator::ControlSignal::Stop;
                            match transaction_generator_handle.send(control_signal) {
                                Ok(()) => respond_result!(req, true, "ok"),
                                Err(e) => respond_result!(
                                    req,
                                    false,
                                    format!(
                                        "error sending control signal to transaction generator: {}",
                                        e
                                    )
                                ),
                            }
                        }
                        "/transaction-generator/step" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let step_count = match params.get("count") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing step count");
                                    return;
                                }
                            };
                            let step_count = match step_count.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing step count: {}", e)
                                    );
                                    return;
                                }
                            };
                            let control_signal =
                                transaction_generator::ControlSignal::Step(step_count);
                            match transaction_generator_handle.send(control_signal) {
                                Ok(()) => respond_result!(req, true, "ok"),
                                Err(e) => respond_result!(
                                    req,
                                    false,
                                    format!(
                                        "error sending control signal to transaction generator: {}",
                                        e
                                    )
                                ),
                            }
                        }
                        "/transaction-generator/set-arrival-distribution" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let distribution = match params.get("distribution") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing distribution");
                                    return;
                                }
                            };
                            let distribution = match distribution.as_ref() {
                                "uniform" => {
                                    let interval = match params.get("interval") {
                                        Some(v) => match v.parse::<u64>() {
                                            Ok(v) => v,
                                            Err(e) => {
                                                respond_result!(
                                                    req,
                                                    false,
                                                    format!("error parsing interval: {}", e)
                                                );
                                                return;
                                            }
                                        },
                                        None => {
                                            respond_result!(req, false, "missing interval");
                                            return;
                                        }
                                    };
                                    transaction_generator::ArrivalDistribution::Uniform(
                                        transaction_generator::UniformArrival { interval },
                                    )
                                }
                                d => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("invalid distribution: {}", d)
                                    );
                                    return;
                                }
                            };
                            let control_signal =
                                transaction_generator::ControlSignal::SetArrivalDistribution(
                                    distribution,
                                );
                            match transaction_generator_handle.send(control_signal) {
                                Ok(()) => respond_result!(req, true, "ok"),
                                Err(e) => respond_result!(
                                    req,
                                    false,
                                    format!(
                                        "error sending control signal to transaction generator: {}",
                                        e
                                    )
                                ),
                            }
                        }
                        "/transaction-generator/set-value-distribution" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let distribution = match params.get("distribution") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing distribution");
                                    return;
                                }
                            };
                            let distribution = match distribution.as_ref() {
                                "uniform" => {
                                    let min = match params.get("min") {
                                        Some(v) => match v.parse::<u64>() {
                                            Ok(v) => v,
                                            Err(e) => {
                                                respond_result!(
                                                    req,
                                                    false,
                                                    format!("error parsing min: {}", e)
                                                );
                                                return;
                                            }
                                        },
                                        None => {
                                            respond_result!(req, false, "missing min");
                                            return;
                                        }
                                    };
                                    let max = match params.get("max") {
                                        Some(v) => match v.parse::<u64>() {
                                            Ok(v) => v,
                                            Err(e) => {
                                                respond_result!(
                                                    req,
                                                    false,
                                                    format!("error parsing max: {}", e)
                                                );
                                                return;
                                            }
                                        },
                                        None => {
                                            respond_result!(req, false, "missing max");
                                            return;
                                        }
                                    };
                                    if min > max {
                                        respond_result!(
                                            req,
                                            false,
                                            "min value is bigger than max value".to_string()
                                        );
                                        return;
                                    }
                                    transaction_generator::ValueDistribution::Uniform(
                                        transaction_generator::UniformValue { min, max },
                                    )
                                }
                                d => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("invalid distribution: {}", d)
                                    );
                                    return;
                                }
                            };
                            let control_signal =
                                transaction_generator::ControlSignal::SetValueDistribution(
                                    distribution,
                                );
                            match transaction_generator_handle.send(control_signal) {
                                Ok(()) => respond_result!(req, true, "ok"),
                                Err(e) => respond_result!(
                                    req,
                                    false,
                                    format!(
                                        "error sending control signal to transaction generator: {}",
                                        e
                                    )
                                ),
                            }
                        }
                        "/transaction-generator/set-transaction-type" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let t = match params.get("type") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing type");
                                    return;
                                }
                            };
                            let control_signal = transaction_generator::ControlSignal::SetTransactionType(
                                    t.clone(),
                                );
                            match transaction_generator_handle.send(control_signal) {
                                Ok(()) => respond_result!(req, true, "ok"),
                                Err(e) => respond_result!(
                                    req,
                                    false,
                                    format!(
                                        "error sending control signal to transaction generator: {}",
                                        e
                                    )
                                ),
                            }
                        }
                        _ => {
                            let content_type =
                                "Content-Type: application/json".parse::<Header>().unwrap();
                            let payload = ApiResponse {
                                success: false,
                                message: "endpoint not found".to_string(),
                            };
                            let resp = Response::from_string(
                                serde_json::to_string_pretty(&payload).unwrap(),
                            )
                            .with_header(content_type)
                            .with_status_code(404);
                            req.respond(resp).unwrap();
                        }
                    }
                });
            }
        });
        info!("API server listening at {}", &addr);
    }
}
