/*
	Copyright 2021 Integritee AG and Supercomputing Systems AG

	Licensed under the Apache License, Version 2.0 (the "License");
	you may not use this file except in compliance with the License.
	You may obtain a copy of the License at

		http://www.apache.org/licenses/LICENSE-2.0

	Unless required by applicable law or agreed to in writing, software
	distributed under the License is distributed on an "AS IS" BASIS,
	WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
	See the License for the specific language governing permissions and
	limitations under the License.

*/

#![cfg_attr(test, feature(assert_matches))]

#[cfg(feature = "teeracle")]
use crate::teeracle::{schedule_periodic_reregistration_thread, start_periodic_market_update};

#[cfg(not(feature = "dcap"))]
use crate::utils::check_files;

use crate::{
	account_funding::{setup_account_funding, EnclaveAccountInfoProvider},
	config::Config,
	error::Error,
	globals::tokio_handle::{GetTokioHandle, GlobalTokioHandle},
	initialized_service::{
		start_is_initialized_server, InitializationHandler, IsInitialized, TrackInitialization,
	},
	ocall_bridge::{
		bridge_api::Bridge as OCallBridge, component_factory::OCallBridgeComponentFactory,
	},
	parentchain_handler::{HandleParentchain, ParentchainHandler},
	prometheus_metrics::{start_metrics_server, EnclaveMetricsReceiver, MetricsHandler},
	sidechain_setup::{sidechain_init_block_production, sidechain_start_untrusted_rpc_server},
	sync_block_broadcaster::SyncBlockBroadcaster,
	utils::extract_shard,
	worker::Worker,
	worker_peers_updater::WorkerPeersUpdater,
};
use base58::ToBase58;
use clap::{load_yaml, App};
use codec::{Decode, Encode};
use enclave::{
	api::enclave_init,
	tls_ra::{enclave_request_state_provisioning, enclave_run_state_provisioning_server},
};
use ita_stf::{Getter, TrustedGetter};
use itc_rpc_client::direct_client::DirectClient;
use itp_enclave_api::{
	direct_request::DirectRequest,
	enclave_base::EnclaveBase,
	remote_attestation::{RemoteAttestation, TlsRemoteAttestation},
	sidechain::Sidechain,
	stf_task_handler::StfTaskHandler,
	teeracle_api::TeeracleApi,
	Enclave,
};
use itp_node_api::{
	api_client::{AccountApi, PalletTeerexApi, ParentchainApi},
	metadata::NodeMetadata,
	node_api_factory::{CreateNodeApi, NodeApiFactory},
};
use itp_settings::worker_mode::{ProvideWorkerMode, WorkerMode, WorkerModeProvider};
use itp_stf_primitives::types::KeyPair;

#[cfg(feature = "dcap")]
use itp_utils::hex::hex_encode;

use its_peer_fetch::{
	block_fetch_client::BlockFetcher, untrusted_peer_fetch::UntrustedPeerFetcher,
};
use its_primitives::types::block::SignedBlock as SignedSidechainBlock;
use its_storage::{interface::FetchBlocks, BlockPruner, SidechainStorageLock};
use lc_data_providers::DataProviderConfig;
use litentry_primitives::{Identity, ParentchainHeader as Header, UserShieldingKeyType};
use log::*;
use my_node_runtime::{Hash, RuntimeEvent};
use serde_json::Value;
use sgx_types::*;
use substrate_api_client::{
	rpc::HandleSubscription, serde_impls::StorageKey, storage_key, GetHeader, GetStorage,
	SubmitAndWatchUntilSuccess, SubscribeChain, SubscribeEvents,
};

#[cfg(feature = "dcap")]
use sgx_verify::extract_tcb_info_from_raw_dcap_quote;

use sp_core::{
	crypto::{AccountId32, Ss58Codec},
	sr25519::Pair as Sr25519Pair,
	Pair,
};
use sp_keyring::AccountKeyring;
use std::{collections::HashSet, env, fs::File, io::Read, str, sync::Arc, thread, time::Duration};
extern crate config as rs_config;
use sp_runtime::traits::Header as HeaderTrait;
use teerex_primitives::{Enclave as TeerexEnclave, ShardIdentifier};

mod account_funding;
mod config;
mod enclave;
mod error;
mod globals;
mod initialized_service;
mod ocall_bridge;
mod parentchain_handler;
mod prometheus_metrics;
mod setup;
mod sidechain_setup;
mod sync_block_broadcaster;
mod sync_state;
#[cfg(feature = "teeracle")]
mod teeracle;
mod tests;
mod utils;
mod worker;
mod worker_peers_updater;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub type EnclaveWorker =
	Worker<Config, NodeApiFactory, Enclave, InitializationHandler<WorkerModeProvider>>;
pub type Event = substrate_api_client::EventRecord<RuntimeEvent, Hash>;

fn main() {
	// Setup logging
	env_logger::init();

	let yml = load_yaml!("cli.yml");
	let matches = App::from_yaml(yml).get_matches();

	let config = Config::from(&matches);

	GlobalTokioHandle::initialize();

	// log this information, don't println because some python scripts for GA rely on the
	// stdout from the service
	#[cfg(feature = "production")]
	info!("*** Starting service in SGX production mode");
	#[cfg(not(feature = "production"))]
	info!("*** Starting service in SGX debug mode");

	info!("*** Running worker in mode: {:?} \n", WorkerModeProvider::worker_mode());

	let clean_reset = matches.is_present("clean-reset");
	if clean_reset {
		setup::purge_files_from_dir(config.data_dir()).unwrap();
	}

	// build the entire dependency tree
	let tokio_handle = Arc::new(GlobalTokioHandle {});
	let sidechain_blockstorage = Arc::new(
		SidechainStorageLock::<SignedSidechainBlock>::from_base_path(
			config.data_dir().to_path_buf(),
		)
		.unwrap(),
	);
	let node_api_factory =
		Arc::new(NodeApiFactory::new(config.node_url(), AccountKeyring::Alice.pair()));
	let enclave = Arc::new(enclave_init(&config).unwrap());
	let initialization_handler = Arc::new(InitializationHandler::default());
	let worker = Arc::new(EnclaveWorker::new(
		config.clone(),
		enclave.clone(),
		node_api_factory.clone(),
		initialization_handler.clone(),
		HashSet::new(),
	));
	let sync_block_broadcaster =
		Arc::new(SyncBlockBroadcaster::new(tokio_handle.clone(), worker.clone()));
	let peer_updater = Arc::new(WorkerPeersUpdater::new(worker));
	let untrusted_peer_fetcher = UntrustedPeerFetcher::new(node_api_factory.clone());
	let peer_sidechain_block_fetcher =
		Arc::new(BlockFetcher::<SignedSidechainBlock, _>::new(untrusted_peer_fetcher));
	let enclave_metrics_receiver = Arc::new(EnclaveMetricsReceiver {});

	// initialize o-call bridge with a concrete factory implementation
	OCallBridge::initialize(Arc::new(OCallBridgeComponentFactory::new(
		node_api_factory.clone(),
		sync_block_broadcaster,
		enclave.clone(),
		sidechain_blockstorage.clone(),
		peer_updater,
		peer_sidechain_block_fetcher,
		tokio_handle.clone(),
		enclave_metrics_receiver,
	)));

	let quoting_enclave_target_info = match enclave.qe_get_target_info() {
		Ok(target_info) => Some(target_info),
		Err(e) => {
			warn!("Setting up DCAP - qe_get_target_info failed with error: {:#?}, continuing.", e);
			None
		},
	};
	let quote_size = match enclave.qe_get_quote_size() {
		Ok(size) => Some(size),
		Err(e) => {
			warn!("Setting up DCAP - qe_get_quote_size failed with error: {:#?}, continuing.", e);
			None
		},
	};

	let data_provider_config = get_data_provider_config(&config);

	if let Some(run_config) = config.run_config() {
		let shard = extract_shard(run_config.shard(), enclave.as_ref());

		println!("Worker Config: {:?}", config);

		// litentry: start the mock-server if enabled
		if config.enable_mock_server {
			let trusted_server_url = format!("wss://localhost:{}", config.trusted_worker_port);
			let mock_server_port = config
				.try_parse_mock_server_port()
				.expect("mock server port to be a valid port number");
			thread::spawn(move || {
				info!("*** Starting mock server");
				let getter = Arc::new(move |who: &Sr25519Pair| {
					let client = DirectClient::new(trusted_server_url.clone());
					let key_getter = Getter::from(
						TrustedGetter::user_shielding_key(Identity::Substrate(who.public().into()))
							.sign(&KeyPair::Sr25519(Box::new(who.clone()))),
					);
					client
						.get_state(shard, &key_getter)
						.and_then(|n| UserShieldingKeyType::decode(&mut n.as_slice()).ok())
						.unwrap_or_default()
				});
				let _ = lc_mock_server::run(getter, mock_server_port);
			});
		}

		if clean_reset {
			setup::initialize_shard_and_keys(enclave.as_ref(), &shard).unwrap();
		}

		let node_api =
			node_api_factory.create_api().expect("Failed to create parentchain node API");

		if run_config.request_state() {
			sync_state::sync_state::<_, _, WorkerModeProvider>(
				&node_api,
				&shard,
				enclave.as_ref(),
				run_config.skip_ra(),
			);
		}

		start_worker::<_, _, _, _, WorkerModeProvider>(
			config,
			&shard,
			&data_provider_config,
			enclave,
			sidechain_blockstorage,
			node_api,
			tokio_handle,
			initialization_handler,
			quoting_enclave_target_info,
			quote_size,
		);
	} else if let Some(smatches) = matches.subcommand_matches("request-state") {
		println!("*** Requesting state from a registered worker \n");
		let node_api =
			node_api_factory.create_api().expect("Failed to create parentchain node API");
		sync_state::sync_state::<_, _, WorkerModeProvider>(
			&node_api,
			&extract_shard(smatches.value_of("shard"), enclave.as_ref()),
			enclave.as_ref(),
			smatches.is_present("skip-ra"),
		);
	} else if matches.is_present("shielding-key") {
		setup::generate_shielding_key_file(enclave.as_ref());
	} else if matches.is_present("signing-key") {
		setup::generate_signing_key_file(enclave.as_ref());
		let tee_accountid = enclave_account(enclave.as_ref());
		println!("Enclave account: {:}", &tee_accountid.to_ss58check());
	} else if matches.is_present("dump-ra") {
		info!("*** Perform RA and dump cert to disk");
		#[cfg(not(feature = "dcap"))]
		enclave.dump_ias_ra_cert_to_disk().unwrap();
		#[cfg(feature = "dcap")]
		{
			let skip_ra = false;
			let dcap_quote = enclave.generate_dcap_ra_quote(skip_ra).unwrap();
			let (fmspc, _tcb_info) = extract_tcb_info_from_raw_dcap_quote(&dcap_quote).unwrap();
			enclave.dump_dcap_collateral_to_disk(fmspc).unwrap();
			enclave.dump_dcap_ra_cert_to_disk().unwrap();
		}
	} else if matches.is_present("mrenclave") {
		println!("{}", enclave.get_mrenclave().unwrap().encode().to_base58());
	} else if let Some(sub_matches) = matches.subcommand_matches("init-shard") {
		setup::init_shard(
			enclave.as_ref(),
			&extract_shard(sub_matches.value_of("shard"), enclave.as_ref()),
		);
	} else if let Some(sub_matches) = matches.subcommand_matches("test") {
		if sub_matches.is_present("provisioning-server") {
			println!("*** Running Enclave MU-RA TLS server\n");
			enclave_run_state_provisioning_server(
				enclave.as_ref(),
				sgx_quote_sign_type_t::SGX_UNLINKABLE_SIGNATURE,
				quoting_enclave_target_info.as_ref(),
				quote_size.as_ref(),
				&config.mu_ra_url(),
				sub_matches.is_present("skip-ra"),
			);
			println!("[+] Done!");
		} else if sub_matches.is_present("provisioning-client") {
			println!("*** Running Enclave MU-RA TLS client\n");
			let shard = extract_shard(sub_matches.value_of("shard"), enclave.as_ref());
			enclave_request_state_provisioning(
				enclave.as_ref(),
				sgx_quote_sign_type_t::SGX_UNLINKABLE_SIGNATURE,
				&config.mu_ra_url_external(),
				&shard,
				sub_matches.is_present("skip-ra"),
			)
			.unwrap();
			println!("[+] Done!");
		} else {
			tests::run_enclave_tests(sub_matches);
		}
	} else if let Some(sub_matches) = matches.subcommand_matches("migrate-shard") {
		// This subcommand `migrate-shard` is only used for manual testing. Maybe deleted later.
		let old_shard = sub_matches
			.value_of("old-shard")
			.map(|value| {
				let mut shard = [0u8; 32];
				hex::decode_to_slice(value, &mut shard)
					.expect("shard must be hex encoded without 0x");
				ShardIdentifier::from_slice(&shard)
			})
			.unwrap();

		let new_shard: ShardIdentifier = sub_matches
			.value_of("new-shard")
			.map(|value| {
				let mut shard = [0u8; 32];
				hex::decode_to_slice(value, &mut shard)
					.expect("shard must be hex encoded without 0x");
				ShardIdentifier::from_slice(&shard)
			})
			.unwrap();

		if old_shard == new_shard {
			println!("old_shard should not be the same as new_shard");
		} else {
			setup::migrate_shard(enclave.as_ref(), &old_shard, &new_shard);
		}
	} else {
		println!("For options: use --help");
	}
}

/// FIXME: needs some discussion (restructuring?)
#[allow(clippy::too_many_arguments)]
fn start_worker<E, T, D, InitializationHandler, WorkerModeProvider>(
	config: Config,
	shard: &ShardIdentifier,
	data_provider_config: &DataProviderConfig,
	enclave: Arc<E>,
	sidechain_storage: Arc<D>,
	node_api: ParentchainApi,
	tokio_handle_getter: Arc<T>,
	initialization_handler: Arc<InitializationHandler>,
	quoting_enclave_target_info: Option<sgx_target_info_t>,
	quote_size: Option<u32>,
) where
	T: GetTokioHandle,
	E: EnclaveBase
		+ DirectRequest
		+ Sidechain
		+ RemoteAttestation
		+ TlsRemoteAttestation
		+ TeeracleApi
		+ StfTaskHandler
		+ Clone,
	D: BlockPruner + FetchBlocks<SignedSidechainBlock> + Sync + Send + 'static,
	InitializationHandler: TrackInitialization + IsInitialized + Sync + Send + 'static,
	WorkerModeProvider: ProvideWorkerMode,
{
	let run_config = config.run_config().clone().expect("Run config missing");
	let skip_ra = run_config.skip_ra();

	println!("Integritee Worker v{}", VERSION);
	info!("starting worker on shard {}", shard.encode().to_base58());
	// ------------------------------------------------------------------------
	// check for required files
	if !skip_ra {
		#[cfg(not(feature = "dcap"))]
		check_files();
	}
	// ------------------------------------------------------------------------
	// initialize the enclave
	let mrenclave = enclave.get_mrenclave().unwrap();
	println!("MRENCLAVE={}", mrenclave.to_base58());
	println!("MRENCLAVE in hex {:?}", hex::encode(mrenclave));

	// ------------------------------------------------------------------------
	// let new workers call us for key provisioning
	println!("MU-RA server listening on {}", config.mu_ra_url());
	let is_development_mode = run_config.dev();
	let ra_url = config.mu_ra_url();
	let enclave_api_key_prov = enclave.clone();
	thread::spawn(move || {
		enclave_run_state_provisioning_server(
			enclave_api_key_prov.as_ref(),
			sgx_quote_sign_type_t::SGX_UNLINKABLE_SIGNATURE,
			quoting_enclave_target_info.as_ref(),
			quote_size.as_ref(),
			&ra_url,
			skip_ra,
		);
		info!("State provisioning server stopped.");
	});

	let tokio_handle = tokio_handle_getter.get_handle();

	#[cfg(feature = "teeracle")]
	let teeracle_tokio_handle = tokio_handle.clone();

	// ------------------------------------------------------------------------
	// Get the public key of our TEE.
	let tee_accountid = enclave_account(enclave.as_ref());
	println!("Enclave account {:} ", &tee_accountid.to_ss58check());

	// ------------------------------------------------------------------------
	// Start `is_initialized` server.
	let untrusted_http_server_port = config
		.try_parse_untrusted_http_server_port()
		.expect("untrusted http server port to be a valid port number");
	let initialization_handler_clone = initialization_handler.clone();
	tokio_handle.spawn(async move {
		if let Err(e) =
			start_is_initialized_server(initialization_handler_clone, untrusted_http_server_port)
				.await
		{
			error!("Unexpected error in `is_initialized` server: {:?}", e);
		}
	});

	// ------------------------------------------------------------------------
	// Start prometheus metrics server.
	if config.enable_metrics_server() {
		let enclave_wallet =
			Arc::new(EnclaveAccountInfoProvider::new(node_api.clone(), tee_accountid.clone()));
		let metrics_handler = Arc::new(MetricsHandler::new(enclave_wallet));
		let metrics_server_port = config
			.try_parse_metrics_server_port()
			.expect("metrics server port to be a valid port number");
		tokio_handle.spawn(async move {
			if let Err(e) = start_metrics_server(metrics_handler, metrics_server_port).await {
				error!("Unexpected error in Prometheus metrics server: {:?}", e);
			}
		});
	}

	// ------------------------------------------------------------------------
	// Start trusted worker rpc server
	if WorkerModeProvider::worker_mode() == WorkerMode::Sidechain
		|| WorkerModeProvider::worker_mode() == WorkerMode::OffChainWorker
	{
		let direct_invocation_server_addr = config.trusted_worker_url_internal();
		let enclave_for_direct_invocation = enclave.clone();
		thread::spawn(move || {
			println!(
				"[+] Trusted RPC direct invocation server listening on {}",
				direct_invocation_server_addr
			);
			enclave_for_direct_invocation
				.init_direct_invocation_server(direct_invocation_server_addr)
				.unwrap();
			println!("[+] RPC direct invocation server shut down");
		});
	}

	// ------------------------------------------------------------------------
	// Start untrusted worker rpc server.
	// i.e move sidechain block importing to trusted worker.
	if WorkerModeProvider::worker_mode() == WorkerMode::Sidechain {
		sidechain_start_untrusted_rpc_server(
			&config,
			enclave.clone(),
			sidechain_storage.clone(),
			tokio_handle,
		);
	}

	// ------------------------------------------------------------------------
	// Init parentchain specific stuff. Needed for parentchain communication.
	let parentchain_handler = Arc::new(
		ParentchainHandler::new_with_automatic_light_client_allocation(
			node_api.clone(),
			enclave.clone(),
		)
		.unwrap(),
	);
	let last_synced_header = parentchain_handler.init_parentchain_components().unwrap();
	info!("Last synced parachain block = {:?}", &last_synced_header.number);
	let nonce = node_api.get_nonce_of(&tee_accountid).unwrap();
	info!("Enclave nonce = {:?}", nonce);
	enclave
		.set_nonce(nonce)
		.expect("Could not set nonce of enclave. Returning here...");

	let metadata = node_api.metadata().clone();
	let runtime_spec_version = node_api.runtime_version().spec_version;
	let runtime_transaction_version = node_api.runtime_version().transaction_version;
	enclave
		.set_node_metadata(
			NodeMetadata::new(metadata, runtime_spec_version, runtime_transaction_version).encode(),
		)
		.expect("Could not set the node metadata in the enclave");

	#[cfg(feature = "dcap")]
	register_collateral(&node_api, &*enclave, &tee_accountid, is_development_mode, skip_ra);

	let trusted_url = config.trusted_worker_url_external();

	#[cfg(feature = "attesteer")]
	fetch_marblerun_events_every_hour(
		node_api.clone(),
		enclave.clone(),
		tee_accountid.clone(),
		is_development_mode,
		trusted_url.clone(),
		run_config.marblerun_base_url().to_string(),
	);

	// ------------------------------------------------------------------------
	// Perform a remote attestation and get an unchecked extrinsic back.

	if skip_ra {
		println!(
			"[!] skipping remote attestation. Registering enclave without attestation report."
		);
	} else {
		println!("[!] creating remote attestation report and create enclave register extrinsic.");
	};

	// clones because of the move
	let enclave2 = enclave.clone();
	let node_api2 = node_api.clone();
	let tee_accountid2 = tee_accountid.clone();
	let trusted_url2 = trusted_url.clone();
	#[cfg(feature = "dcap")]
	enclave2.set_sgx_qpl_logging().expect("QPL logging setup failed");
	#[cfg(not(feature = "dcap"))]
	let register_xt = move || enclave2.generate_ias_ra_extrinsic(&trusted_url2, skip_ra).unwrap();
	#[cfg(feature = "dcap")]
	let register_xt = move || enclave2.generate_dcap_ra_extrinsic(&trusted_url, skip_ra).unwrap();

	let _send_register_xt = move || {
		send_extrinsic(register_xt(), &node_api2, &tee_accountid2.clone(), is_development_mode)
	};

	#[cfg(not(feature = "dcap"))]
	let xt = enclave.generate_ias_ra_extrinsic(&trusted_url, skip_ra).unwrap();
	#[cfg(feature = "dcap")]
	let xt = enclave.generate_dcap_ra_extrinsic(&trusted_url, skip_ra).unwrap();

	let mut xthex = hex::encode(xt.clone());
	xthex.insert_str(0, "0x");

	// Account funds
	if let Err(x) =
		setup_account_funding(&node_api, &tee_accountid, xthex.into(), is_development_mode)
	{
		error!("Starting worker failed: {:?}", x);
		// Return without registering the enclave. This will fail and the transaction will be banned for 30min.
		return
	}

	let mut register_enclave_xt_header: Option<Header> = None;
	let mut we_are_primary_validateer: bool = false;

	// litentry, Check if the enclave is already registered
	match node_api.get_keys(storage_key("Teerex", "EnclaveRegistry"), None) {
		Ok(Some(keys)) => {
			let trusted_url = trusted_url.as_bytes().to_vec();
			let mrenclave = mrenclave.to_vec();
			let mut found = false;
			for key in keys {
				let key = if key.starts_with("0x") {
					let bytes = &key.as_bytes()[b"0x".len()..];
					hex::decode(bytes).unwrap()
				} else {
					hex::decode(key.as_bytes()).unwrap()
				};
				match node_api.get_storage_by_key_hash::<TeerexEnclave<AccountId32, Vec<u8>>>(
					StorageKey(key.clone()),
					None,
				) {
					Ok(Some(value)) => {
						if value.mr_enclave.to_vec() == mrenclave && value.url == trusted_url {
							// After calling the perform_ra function, the nonce will be incremented by 1,
							// so enclave is already registered, we should reset the nonce_cache
							enclave
								.set_nonce(nonce)
								.expect("Could not set nonce of enclave. Returning here...");
							found = true;
							info!("fond enclave: {:?}", value);
							break
						}
					},
					Ok(None) => {
						warn!("not found from key: {:?}", key);
					},
					Err(_) => {},
				}
			}
			if !found {
				println!("[>] Register the enclave (send the extrinsic)");
				let register_enclave_xt_hash =
					send_extrinsic(xt, &node_api, &tee_accountid, is_development_mode);
				println!("[<] Extrinsic got finalized. Hash: {:?}\n", register_enclave_xt_hash);
				register_enclave_xt_header = node_api.get_header(register_enclave_xt_hash).unwrap();
			}
		},
		_ => panic!("unknown error"),
	}

	if let Some(register_enclave_xt_header) = register_enclave_xt_header.clone() {
		we_are_primary_validateer =
			check_we_are_primary_validateer(&node_api, &register_enclave_xt_header).unwrap();
	}

	if we_are_primary_validateer {
		println!("[+] We are the primary validateer");
	} else {
		println!("[+] We are NOT the primary validateer");
	}

	initialization_handler.registered_on_parentchain();

	// ------------------------------------------------------------------------
	// Start stf task handler thread
	let enclave_api_stf_task_handler = enclave.clone();
	let data_provider_config = data_provider_config.clone();
	thread::spawn(move || {
		enclave_api_stf_task_handler.run_stf_task_handler(data_provider_config).unwrap();
	});

	// ------------------------------------------------------------------------
	// initialize teeracle interval
	#[cfg(feature = "teeracle")]
	if WorkerModeProvider::worker_mode() == WorkerMode::Teeracle {
		schedule_periodic_reregistration_thread(
			_send_register_xt,
			run_config.reregister_teeracle_interval(),
		);

		start_periodic_market_update(
			&node_api,
			run_config.teeracle_update_interval(),
			enclave.as_ref(),
			&teeracle_tokio_handle,
		);
	}

	if WorkerModeProvider::worker_mode() != WorkerMode::Teeracle {
		let parentchain_start_block = config
			.try_parse_parentchain_start_block()
			.expect("parentchain start block to be a valid number");
		println!("*** [+] Finished syncing light client, syncing parentchain...");
		println!(
			"*** [+] last_synced_header: {}, config.parentchain_start_block: {}",
			last_synced_header.number, parentchain_start_block
		);

		// Syncing all parentchain blocks, this might take a while..
		let mut last_synced_header = match parentchain_handler
			.sync_parentchain(last_synced_header, parentchain_start_block)
		{
			Ok(value) => value,
			Err(error) => {
				println!("sync_parentchain error: {:?}", error);
				Header {
					parent_hash: Default::default(),
					number: 0,
					extrinsics_root: Default::default(),
					state_root: Default::default(),
					digest: Default::default(),
				}
			},
		};

		// ------------------------------------------------------------------------
		// Initialize the sidechain
		if WorkerModeProvider::worker_mode() == WorkerMode::Sidechain {
			last_synced_header = match sidechain_init_block_production(
				enclave,
				register_enclave_xt_header,
				we_are_primary_validateer,
				parentchain_handler.clone(),
				sidechain_storage,
				&last_synced_header,
				parentchain_start_block,
			) {
				Ok(value) => value,
				Err(error) => {
					println!("sidechain_init_block_production error: {:?}", error);
					Header {
						parent_hash: Default::default(),
						number: 0,
						extrinsics_root: Default::default(),
						state_root: Default::default(),
						digest: Default::default(),
					}
				},
			};
		}

		// ------------------------------------------------------------------------
		// start parentchain syncing loop (subscribe to header updates)
		thread::Builder::new()
			.name("parentchain_sync_loop".to_owned())
			.spawn(move || {
				if let Err(e) =
					subscribe_to_parentchain_new_headers(parentchain_handler, last_synced_header)
				{
					error!("Parentchain block syncing terminated with a failure: {:?}", e);
				}
				println!("[!] Parentchain block syncing has terminated");
			})
			.unwrap();
	}

	// ------------------------------------------------------------------------
	if WorkerModeProvider::worker_mode() == WorkerMode::Sidechain {
		spawn_worker_for_shard_polling(shard, node_api.clone(), initialization_handler);
	}

	// ------------------------------------------------------------------------
	// Subscribe to events and print them.
	println!("*** Subscribing to events");
	let mut subscription = node_api.subscribe_events().unwrap();
	println!("[+] Subscribed to events. waiting...");
	loop {
		if let Some(Ok(events)) = subscription.next_event::<RuntimeEvent, Hash>() {
			print_events(events)
		}
	}
}

/// Start polling loop to wait until we have a worker for a shard registered on
/// the parentchain (TEEREX WorkerForShard). This is the pre-requisite to be
/// considered initialized and ready for the next worker to start (in sidechain mode only).
/// considered initialized and ready for the next worker to start.
fn spawn_worker_for_shard_polling<InitializationHandler>(
	shard: &ShardIdentifier,
	node_api: ParentchainApi,
	initialization_handler: Arc<InitializationHandler>,
) where
	InitializationHandler: TrackInitialization + Sync + Send + 'static,
{
	let shard_for_initialized = *shard;
	thread::spawn(move || {
		const POLL_INTERVAL_SECS: u64 = 2;

		loop {
			info!("Polling for worker for shard ({} seconds interval)", POLL_INTERVAL_SECS);
			if let Ok(Some(_)) = node_api.worker_for_shard(&shard_for_initialized, None) {
				// Set that the service is initialized.
				initialization_handler.worker_for_shard_registered();
				println!("[+] Found `WorkerForShard` on parentchain state");
				break
			}
			thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));
		}
	});
}

fn print_events(events: Vec<Event>) {
	for evr in &events {
		debug!("Decoded: phase = {:?}, event = {:?}", evr.phase, evr.event);
		match &evr.event {
			RuntimeEvent::Balances(be) => {
				info!("[+] Received balances event");
				debug!("{:?}", be);
				match &be {
					pallet_balances::Event::Transfer {
						from: transactor,
						to: dest,
						amount: value,
					} => {
						debug!("    Transactor:  {:?}", transactor.to_ss58check());
						debug!("    Destination: {:?}", dest.to_ss58check());
						debug!("    Value:       {:?}", value);
					},
					_ => {
						trace!("Ignoring unsupported balances event");
					},
				}
			},
			RuntimeEvent::Teerex(re) => {
				debug!("{:?}", re);
				match &re {
					my_node_runtime::pallet_teerex::Event::AddedEnclave(sender, worker_url) => {
						println!("[+] Received AddedEnclave event");
						println!("    Sender (Worker):  {:?}", sender);
						println!("    Registered URL: {:?}", str::from_utf8(worker_url).unwrap());
					},
					my_node_runtime::pallet_teerex::Event::Forwarded(shard) => {
						println!(
							"[+] Received trusted call for shard {}",
							shard.encode().to_base58()
						);
					},
					my_node_runtime::pallet_teerex::Event::ProcessedParentchainBlock(
						sender,
						block_hash,
						merkle_root,
						block_number,
					) => {
						info!("[+] Received ProcessedParentchainBlock event");
						debug!("    From:    {:?}", sender);
						debug!("    Block Hash: {:?}", hex::encode(block_hash));
						debug!("    Merkle Root: {:?}", hex::encode(merkle_root));
						debug!("    Block Number: {:?}", block_number);
					},
					my_node_runtime::pallet_teerex::Event::ShieldFunds(incognito_account) => {
						info!("[+] Received ShieldFunds event");
						debug!("    For:    {:?}", incognito_account);
					},
					my_node_runtime::pallet_teerex::Event::UnshieldedFunds(incognito_account) => {
						info!("[+] Received UnshieldedFunds event");
						debug!("    For:    {:?}", incognito_account);
					},
					_ => {
						trace!("Ignoring unsupported pallet_teerex event");
					},
				}
			},
			#[cfg(feature = "teeracle")]
			RuntimeEvent::Teeracle(re) => {
				debug!("{:?}", re);
				match &re {
					my_node_runtime::pallet_teeracle::Event::ExchangeRateUpdated(
						source,
						currency,
						new_value,
					) => {
						println!("[+] Received ExchangeRateUpdated event");
						println!("    Data source:  {}", source);
						println!("    Currency:  {}", currency);
						println!("    Exchange rate: {:?}", new_value);
					},
					my_node_runtime::pallet_teeracle::Event::ExchangeRateDeleted(
						source,
						currency,
					) => {
						println!("[+] Received ExchangeRateDeleted event");
						println!("    Data source:  {}", source);
						println!("    Currency:  {}", currency);
					},
					my_node_runtime::pallet_teeracle::Event::AddedToWhitelist(
						source,
						mrenclave,
					) => {
						println!("[+] Received AddedToWhitelist event");
						println!("    Data source:  {}", source);
						println!("    Currency:  {:?}", mrenclave);
					},
					my_node_runtime::pallet_teeracle::Event::RemovedFromWhitelist(
						source,
						mrenclave,
					) => {
						println!("[+] Received RemovedFromWhitelist event");
						println!("    Data source:  {}", source);
						println!("    Currency:  {:?}", mrenclave);
					},
					_ => {
						trace!("Ignoring unsupported pallet_teeracle event");
					},
				}
			},
			#[cfg(feature = "sidechain")]
			RuntimeEvent::Sidechain(re) => match &re {
				my_node_runtime::pallet_sidechain::Event::ProposedSidechainBlock(
					sender,
					payload,
				) => {
					info!("[+] Received ProposedSidechainBlock event");
					debug!("    From:    {:?}", sender);
					debug!("    Payload: {:?}", hex::encode(payload));
				},
				_ => {
					trace!("Ignoring unsupported pallet_sidechain event");
				},
			},
			_ => {
				trace!("Ignoring event {:?}", evr);
			},
		}
	}
}

#[cfg(feature = "attesteer")]
fn fetch_marblerun_events_every_hour<E>(
	api: ParentchainApi,
	enclave: Arc<E>,
	accountid: AccountId32,
	is_development_mode: bool,
	url: String,
	marblerun_base_url: String,
) where
	E: RemoteAttestation + Clone + Sync + Send + 'static,
{
	let enclave = enclave.clone();
	let handle = thread::spawn(move || {
		const POLL_INTERVAL_5_MINUTES_IN_SECS: u64 = 5 * 60;
		loop {
			info!("Polling marblerun events for quotes to register");
			register_quotes_from_marblerun(
				&api,
				enclave.clone(),
				&accountid,
				is_development_mode,
				url.clone(),
				&marblerun_base_url,
			);

			thread::sleep(Duration::from_secs(POLL_INTERVAL_5_MINUTES_IN_SECS));
		}
	});

	handle.join().unwrap()
}
#[cfg(feature = "attesteer")]
fn register_quotes_from_marblerun(
	api: &ParentchainApi,
	enclave: Arc<dyn RemoteAttestation>,
	accountid: &AccountId32,
	is_development_mode: bool,
	url: String,
	marblerun_base_url: &str,
) {
	let enclave = enclave.as_ref();
	let events = prometheus_metrics::fetch_marblerun_events(marblerun_base_url)
		.map_err(|e| {
			info!("Fetching events from Marblerun failed with: {:?}, continuing with 0 events.", e);
		})
		.unwrap_or_default();
	let quotes: Vec<&[u8]> =
		events.iter().map(|event| event.get_quote_without_prepended_bytes()).collect();

	for quote in quotes {
		match enclave.generate_dcap_ra_extrinsic_from_quote(url.clone(), &quote) {
			Ok(xt) => {
				send_extrinsic(xt, api, accountid, is_development_mode);
			},
			Err(e) => {
				error!("Extracting information from quote failed: {}", e)
			},
		}
	}
}
#[cfg(feature = "dcap")]
fn register_collateral(
	api: &ParentchainApi,
	enclave: &dyn RemoteAttestation,
	accountid: &AccountId32,
	is_development_mode: bool,
	skip_ra: bool,
) {
	//TODO generate_dcap_ra_quote() does not really need skip_ra, rethink how many layers skip_ra should be passed along
	if !skip_ra {
		let dcap_quote = enclave.generate_dcap_ra_quote(skip_ra).unwrap();
		let (fmspc, _tcb_info) = extract_tcb_info_from_raw_dcap_quote(&dcap_quote).unwrap();
		println!("[>] DCAP setup: register QE collateral");
		let uxt = enclave.generate_register_quoting_enclave_extrinsic(fmspc).unwrap();
		send_extrinsic(uxt, api, accountid, is_development_mode);

		println!("[>] DCAP setup: register TCB info");
		let uxt = enclave.generate_register_tcb_info_extrinsic(fmspc).unwrap();
		send_extrinsic(uxt, api, accountid, is_development_mode);
	}
}

fn send_extrinsic(
	extrinsic: Vec<u8>,
	api: &ParentchainApi,
	accountid: &AccountId32,
	is_development_mode: bool,
) -> Option<Hash> {
	// Account funds
	if let Err(x) = setup_account_funding(api, accountid, extrinsic.clone(), is_development_mode) {
		error!("Starting worker failed: {:?}", x);
		// Return without registering the enclave. This will fail and the transaction will be banned for 30min.
		return None
	}

	println!("[>] send extrinsic");

	match api.submit_and_watch_opaque_extrinsic_until_success(extrinsic.into(), true) {
		Ok(xt_report) => {
			let register_qe_block_hash = xt_report.block_hash;
			println!("[<] Extrinsic got finalized. Block hash: {:?}\n", register_qe_block_hash);
			register_qe_block_hash
		},
		Err(e) => {
			error!("ExtrinsicFailed {:?}", e);
			None
		},
	}
}

/// Subscribe to the node API finalized heads stream and trigger a parent chain sync
/// upon receiving a new header.
fn subscribe_to_parentchain_new_headers<E: EnclaveBase + Sidechain>(
	parentchain_handler: Arc<ParentchainHandler<ParentchainApi, E>>,
	mut last_synced_header: Header,
) -> Result<(), Error> {
	// TODO: this should be implemented by parentchain_handler directly, and not via
	// exposed parentchain_api
	let mut subscription = parentchain_handler
		.parentchain_api()
		.subscribe_finalized_heads()
		.map_err(Error::ApiClient)?;

	// TODO(Kai@Litentry):
	// originally we had an outer loop to try to handle the disconnection,
	// see https://github.com/litentry/litentry-parachain/commit/b8059d0fad928e4bba99178451cd0d473791c437
	// but I reverted it because:
	// - no graceful shutdown, we could have many mpsc channel when it doesn't go right
	// - we might have multiple `sync_parentchain` running concurrently, which causes chaos in enclave side
	// - I still feel it's only a workaround, not a perfect solution
	//
	// TODO: now the sync will panic if disconnected - it heavily relys on the worker-restart to work (even manually)
	loop {
		let new_header = subscription
			.next()
			.ok_or(Error::ApiSubscriptionDisconnected)?
			.map_err(|e| Error::ApiClient(e.into()))?;

		println!(
			"[+] Received finalized header update ({}), syncing parent chain...",
			new_header.number
		);

		// the overriden_start_block shouldn't matter here
		last_synced_header = parentchain_handler.sync_parentchain(last_synced_header, 0)?;
	}
}

/// Get the public signing key of the TEE.
fn enclave_account<E: EnclaveBase>(enclave_api: &E) -> AccountId32 {
	let tee_public = enclave_api.get_ecc_signing_pubkey().unwrap();
	trace!("[+] Got ed25519 account of TEE = {}", tee_public.to_ss58check());
	AccountId32::from(*tee_public.as_array_ref())
}

/// Checks if we are the first validateer to register on the parentchain.
fn check_we_are_primary_validateer(
	node_api: &ParentchainApi,
	register_enclave_xt_header: &Header,
) -> Result<bool, Error> {
	let enclave_count_of_previous_block =
		node_api.enclave_count(Some(*register_enclave_xt_header.parent_hash()))?;
	Ok(enclave_count_of_previous_block == 0)
}

fn get_data_provider_config(config: &Config) -> DataProviderConfig {
	let built_in_modes = vec!["dev", "staging", "prod", "mock"];
	let built_in_config: Value =
		serde_json::from_slice(include_bytes!("running-mode-config.json")).unwrap();

	let mut data_provider_config = if built_in_modes.contains(&config.running_mode.as_str()) {
		let config = built_in_config.get(config.running_mode.as_str()).unwrap();
		serde_json::from_value::<DataProviderConfig>(config.clone()).unwrap()
	} else {
		let file_path = config.running_mode.as_str();
		let mut file = File::open(file_path)
			.map_err(|e| format!("{:?}, file:{}", e, file_path))
			.unwrap();
		let mut data = String::new();
		file.read_to_string(&mut data).unwrap();
		serde_json::from_str::<DataProviderConfig>(data.as_str()).unwrap()
	};
	if let Ok(v) = env::var("TWITTER_OFFICIAL_URL") {
		data_provider_config.set_twitter_official_url(v);
	}
	if let Ok(v) = env::var("TWITTER_LITENTRY_URL") {
		data_provider_config.set_twitter_litentry_url(v);
	}
	// Bearer Token is as same as App only Access Token on Twitter (https://developer.twitter.com/en/docs/authentication/oauth-2-0/application-only),
	// that is for developers that just need read-only access to public information.
	if let Ok(v) = env::var("TWITTER_AUTH_TOKEN_V2") {
		data_provider_config.set_twitter_auth_token_v2(v);
	}
	if let Ok(v) = env::var("DISCORD_OFFICIAL_URL") {
		data_provider_config.set_discord_official_url(v);
	}
	if let Ok(v) = env::var("DISCORD_LITENTRY_URL") {
		data_provider_config.set_discord_litentry_url(v);
	}
	if let Ok(v) = env::var("DISCORD_AUTH_TOKEN") {
		data_provider_config.set_discord_auth_token(v);
	}
	if let Ok(v) = env::var("ACHAINABLE_URL") {
		data_provider_config.set_achainable_url(v);
	}
	if let Ok(v) = env::var("ACHAINABLE_AUTH_KEY") {
		data_provider_config.set_achainable_auth_key(v);
	}
	if let Ok(v) = env::var("CREDENTIAL_ENDPOINT") {
		data_provider_config.set_credential_endpoint(v);
	}

	data_provider_config
}
