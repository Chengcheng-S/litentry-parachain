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

use crate::{
	error::{Error, Result},
	traits::{StatePostProcessing, StateUpdateProposer, StfUpdateState},
	BatchExecutionResult, ExecutedOperation,
};
use codec::{Decode, Encode};
use ita_stf::{
	hash::{Hash, TrustedOperationOrHash},
	stf_sgx::{shards_key_hash, storage_hashes_to_update_per_shard},
	ParentchainHeader, TrustedCallSigned, TrustedOperation,
};
use itp_node_api::metadata::{provider::AccessNodeMetadata, NodeMetadataTrait};
use itp_ocall_api::{EnclaveAttestationOCallApi, EnclaveOnChainOCallApi};
use itp_sgx_externalities::{SgxExternalitiesTrait, StateHash};
use itp_stf_interface::{
	parentchain_pallet::ParentchainPalletInterface, runtime_upgrade::RuntimeUpgradeInterface,
	ExecuteCall, StateCallInterface, UpdateState,
};
use itp_stf_primitives::types::ShardIdentifier;
use itp_stf_state_handler::{handle_state::HandleState, query_shard_state::QueryShardState};
use itp_time_utils::duration_now;
use itp_types::{storage::StorageEntryVerified, OpaqueCall, H256};
use log::*;
use sp_runtime::traits::Header as HeaderTrait;
use std::{
	collections::BTreeMap, fmt::Debug, marker::PhantomData, sync::Arc, time::Duration, vec,
	vec::Vec,
};

pub struct StfExecutor<OCallApi, StateHandler, NodeMetadataRepository, Stf> {
	ocall_api: Arc<OCallApi>,
	state_handler: Arc<StateHandler>,
	node_metadata_repo: Arc<NodeMetadataRepository>,
	_phantom: PhantomData<Stf>,
}

impl<OCallApi, StateHandler, NodeMetadataRepository, Stf>
	StfExecutor<OCallApi, StateHandler, NodeMetadataRepository, Stf>
where
	OCallApi: EnclaveAttestationOCallApi + EnclaveOnChainOCallApi,
	StateHandler: HandleState<HashType = H256>,
	StateHandler::StateT: SgxExternalitiesTrait + Encode,
	NodeMetadataRepository: AccessNodeMetadata,
	NodeMetadataRepository::MetadataType: NodeMetadataTrait,
	Stf: UpdateState<
			StateHandler::StateT,
			<StateHandler::StateT as SgxExternalitiesTrait>::SgxExternalitiesDiffType,
		> + StateCallInterface<TrustedCallSigned, StateHandler::StateT, NodeMetadataRepository>,
	<StateHandler::StateT as SgxExternalitiesTrait>::SgxExternalitiesDiffType:
		IntoIterator<Item = (Vec<u8>, Option<Vec<u8>>)> + From<BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
	<Stf as StateCallInterface<TrustedCallSigned, StateHandler::StateT, NodeMetadataRepository>>::Error: Debug,
{
	pub fn new(
		ocall_api: Arc<OCallApi>,
		state_handler: Arc<StateHandler>,
		node_metadata_repo: Arc<NodeMetadataRepository>,
	) -> Self {
		StfExecutor { ocall_api, state_handler, node_metadata_repo, _phantom: PhantomData }
	}

	/// Execute a trusted call on the STF
	///
	/// We distinguish between an error in the execution, which maps to `Err` and
	/// an invalid trusted call, which results in `Ok(ExecutionStatus::Failure)`. The latter
	/// can be used to remove the trusted call from a queue. In the former case we might keep the
	/// trusted call and just re-try the operation.
	fn execute_trusted_call_on_stf<PH>(
		&self,
		state: &mut StateHandler::StateT,
		trusted_operation: &TrustedOperation,
		header: &PH,
		shard: &ShardIdentifier,
		post_processing: StatePostProcessing,
	) -> Result<ExecutedOperation>
	where
		PH: HeaderTrait<Hash = H256>,
	{
		debug!("query mrenclave of self");
		let mrenclave = self.ocall_api.get_mrenclave_of_self()?;

		let top_or_hash = TrustedOperationOrHash::from_top(trusted_operation.clone());

		// TODO(Litentry): do we need to send any error notification to parachain?
		let trusted_call = match trusted_operation.to_call().ok_or(Error::InvalidTrustedCallType) {
			Ok(c) => c,
			Err(e) => {
				error!("Error: {:?}", e);
				return Ok(ExecutedOperation::failed(top_or_hash, vec![]))
			},
		};

		if !trusted_call.verify_signature(&mrenclave.m, &shard) {
			error!("TrustedCallSigned: bad signature");
			return Ok(ExecutedOperation::failed(top_or_hash, vec![]))
		}

		// Necessary because light client sync may not be up to date
		// see issue #208
		debug!("Update STF storage!");

		let storage_hashes = <TrustedCallSigned as ExecuteCall<NodeMetadataRepository>>::get_storage_hashes_to_update(trusted_call.clone());
		let update_map = self
			.ocall_api
			.get_multiple_storages_verified(storage_hashes, header)
			.map(into_map)?;

		debug!("Apply state diff with {} entries from parentchain block", update_map.len());
		Stf::apply_state_diff(state, update_map.into());

		debug!("execute on STF, call with nonce {}", trusted_call.nonce);
		let mut extrinsic_call_backs: Vec<OpaqueCall> = Vec::new();
		let rpc_response_value = match Stf::execute_call(
			state,
			shard,
			trusted_call.clone(),
			trusted_operation.hash(),
			&mut extrinsic_call_backs,
			self.node_metadata_repo.clone(),
		) {
			Err(e) => {
				error!("Stf execute failed: {:?}", e);
				return Ok(ExecutedOperation::failed(top_or_hash, extrinsic_call_backs))
			},
			Ok(res) => res,
		};

		let operation_hash = trusted_operation.hash();
		debug!("Operation hash {:?}", operation_hash);

		if let StatePostProcessing::Prune = post_processing {
			state.prune_state_diff();
		}

		Ok(ExecutedOperation::success(operation_hash, top_or_hash, extrinsic_call_backs, rpc_response_value))
	}
}

impl<OCallApi, StateHandler, NodeMetadataRepository, Stf> StfUpdateState
	for StfExecutor<OCallApi, StateHandler, NodeMetadataRepository, Stf>
where
	OCallApi: EnclaveAttestationOCallApi + EnclaveOnChainOCallApi,
	StateHandler: HandleState<HashType = H256> + QueryShardState,
	StateHandler::StateT: SgxExternalitiesTrait + Encode,
	NodeMetadataRepository: AccessNodeMetadata,
	Stf: UpdateState<
			StateHandler::StateT,
			<StateHandler::StateT as SgxExternalitiesTrait>::SgxExternalitiesDiffType,
		> + ParentchainPalletInterface<StateHandler::StateT, ParentchainHeader>,
	<StateHandler::StateT as SgxExternalitiesTrait>::SgxExternalitiesDiffType:
		IntoIterator<Item = (Vec<u8>, Option<Vec<u8>>)>,
	<Stf as ParentchainPalletInterface<StateHandler::StateT, ParentchainHeader>>::Error: Debug,
	<StateHandler::StateT as SgxExternalitiesTrait>::SgxExternalitiesDiffType:
		From<BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
{
	fn update_states(&self, header: &ParentchainHeader) -> Result<()> {
		debug!("Update STF storage upon block import!");
		let storage_hashes = Stf::storage_hashes_to_update_on_block();

		if storage_hashes.is_empty() {
			return Ok(())
		}

		// global requests they are the same for every shard
		let state_diff_update = self
			.ocall_api
			.get_multiple_storages_verified(storage_hashes, header)
			.map(into_map)?;

		// Update parentchain block on all states.
		// TODO: Investigate if this is still necessary. We load and clone the entire state here,
		// which scales badly for increasing state size.
		let shards = self.state_handler.list_shards()?;
		for shard_id in shards {
			let (state_lock, mut state) = self.state_handler.load_for_mutation(&shard_id)?;
			match Stf::update_parentchain_block(&mut state, header.clone()) {
				Ok(_) => {
					self.state_handler.write_after_mutation(state, state_lock, &shard_id)?;
				},
				Err(e) => error!("Could not update parentchain block. {:?}: {:?}", shard_id, e),
			}
		}

		// look for new shards an initialize them
		if let Some(maybe_shards) = state_diff_update.get(&shards_key_hash()) {
			match maybe_shards {
				Some(shards) => {
					let shards: Vec<ShardIdentifier> = Decode::decode(&mut shards.as_slice())?;

					for shard_id in shards {
						let (state_lock, mut state) =
							self.state_handler.load_for_mutation(&shard_id)?;
						trace!("Successfully loaded state, updating states ...");

						// per shard (cid) requests
						let per_shard_hashes = storage_hashes_to_update_per_shard(&shard_id);
						let per_shard_update = self
							.ocall_api
							.get_multiple_storages_verified(per_shard_hashes, header)
							.map(into_map)?;

						Stf::apply_state_diff(&mut state, per_shard_update.into());
						Stf::apply_state_diff(&mut state, state_diff_update.clone().into());
						if let Err(e) = Stf::update_parentchain_block(&mut state, header.clone()) {
							error!("Could not update parentchain block. {:?}: {:?}", shard_id, e)
						}

						self.state_handler.write_after_mutation(state, state_lock, &shard_id)?;
					}
				},
				None => debug!("No shards are on the chain yet"),
			};
		};
		Ok(())
	}
}

impl<OCallApi, StateHandler, NodeMetadataRepository, Stf> StateUpdateProposer
	for StfExecutor<OCallApi, StateHandler, NodeMetadataRepository, Stf>
where
	OCallApi: EnclaveAttestationOCallApi + EnclaveOnChainOCallApi,
	StateHandler: HandleState<HashType = H256>,
	StateHandler::StateT: SgxExternalitiesTrait + Encode + StateHash,
	<StateHandler::StateT as SgxExternalitiesTrait>::SgxExternalitiesType: Encode,
	NodeMetadataRepository: AccessNodeMetadata,
	NodeMetadataRepository::MetadataType: NodeMetadataTrait,
	Stf: UpdateState<
			StateHandler::StateT,
			<StateHandler::StateT as SgxExternalitiesTrait>::SgxExternalitiesDiffType,
		> + StateCallInterface<TrustedCallSigned, StateHandler::StateT, NodeMetadataRepository> + RuntimeUpgradeInterface<StateHandler::StateT>,
	<StateHandler::StateT as SgxExternalitiesTrait>::SgxExternalitiesDiffType:
		IntoIterator<Item = (Vec<u8>, Option<Vec<u8>>)>,
	<StateHandler::StateT as SgxExternalitiesTrait>::SgxExternalitiesDiffType:
		From<BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
	<Stf as StateCallInterface<TrustedCallSigned, StateHandler::StateT, NodeMetadataRepository>>::Error: Debug,
	<Stf as RuntimeUpgradeInterface<StateHandler::StateT>>::Error: Debug,
{
	type Externalities = StateHandler::StateT;

	fn propose_state_update<PH, F>(
		&self,
		trusted_calls: &[TrustedOperation],
		header: &PH,
		shard: &ShardIdentifier,
		max_exec_duration: Duration,
		prepare_state_function: F,
	) -> Result<BatchExecutionResult<Self::Externalities>>
	where
		PH: HeaderTrait<Hash = H256>,
		F: FnOnce(Self::Externalities) -> Self::Externalities,
	{
		let ends_at = duration_now() + max_exec_duration;

		let (state, state_hash_before_execution) = self.state_handler.load_cloned(shard)?;

		// Execute any pre-processing steps.
		let mut state = prepare_state_function(state);
		let mut executed_and_failed_calls = Vec::<ExecutedOperation>::new();

		// TODO: maybe we can move it to `prepare_state_function`. It seems more reasonable.
		let _ = Stf::on_runtime_upgrade(&mut state);

		// Iterate through all calls until time is over.
		for trusted_call_signed in trusted_calls.into_iter() {
			// Break if allowed time window is over.
			if ends_at < duration_now() {
				info!("Aborting execution of trusted calls because slot time is up");
				break
			}

			match self.execute_trusted_call_on_stf(
				&mut state,
				&trusted_call_signed,
				header,
				shard,
				StatePostProcessing::None,
			) {
				Ok(executed_or_failed_call) => {
					executed_and_failed_calls.push(executed_or_failed_call);
				},
				Err(e) => {
					error!("Fatal Error. Failed to attempt call execution: {:?}", e);
				},
			};
		}

		Ok(BatchExecutionResult {
			executed_operations: executed_and_failed_calls,
			state_hash_before_execution,
			state_after_execution: state,
		})
	}
}

fn into_map(
	storage_entries: Vec<StorageEntryVerified<Vec<u8>>>,
) -> BTreeMap<Vec<u8>, Option<Vec<u8>>> {
	storage_entries.into_iter().map(|e| e.into_tuple()).collect()
}
