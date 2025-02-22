// Copyright 2020-2023 Litentry Technologies GmbH.
// This file is part of Litentry.
//
// Litentry is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Litentry is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Litentry.  If not, see <https://www.gnu.org/licenses/>.

use crate::{handler::TaskHandler, EnclaveOnChainOCallApi, StfTaskContext, TrustedCall, H256};
use ita_sgx_runtime::Hash;
use itp_sgx_crypto::{ShieldingCryptoDecrypt, ShieldingCryptoEncrypt};
use itp_sgx_externalities::SgxExternalitiesTrait;
use itp_stf_executor::traits::StfEnclaveSigning;
use itp_stf_state_handler::handle_state::HandleState;
use itp_top_pool_author::traits::AuthorApi;
use lc_credentials::DID;
use lc_data_providers::GLOBAL_DATA_PROVIDER_CONFIG;
use lc_stf_task_sender::AssertionBuildRequest;
use litentry_primitives::{Assertion, ErrorDetail, ErrorString, Identity, VCMPError};
use log::*;
use sp_core::hashing::blake2_256;
use std::{format, sync::Arc, vec::Vec};

pub(crate) struct AssertionHandler<
	K: ShieldingCryptoDecrypt + ShieldingCryptoEncrypt + Clone,
	A: AuthorApi<Hash, Hash>,
	S: StfEnclaveSigning,
	H: HandleState,
	O: EnclaveOnChainOCallApi,
> {
	pub(crate) req: AssertionBuildRequest,
	pub(crate) context: Arc<StfTaskContext<K, A, S, H, O>>,
}

impl<K, A, S, H, O> TaskHandler for AssertionHandler<K, A, S, H, O>
where
	K: ShieldingCryptoDecrypt + ShieldingCryptoEncrypt + Clone,
	A: AuthorApi<Hash, Hash>,
	S: StfEnclaveSigning,
	H: HandleState,
	H::StateT: SgxExternalitiesTrait,
	O: EnclaveOnChainOCallApi,
{
	type Error = VCMPError;
	type Result = (H256, H256, Vec<u8>); // (vc_index, vc_hash, vc_byte_array)

	fn on_process(&self) -> Result<Self::Result, Self::Error> {
		// create the initial credential
		// TODO: maybe we can further simplify this
		let mut credential = match self.req.assertion.clone() {
			Assertion::A1 => lc_assertion_build::a1::build(&self.req),

			Assertion::A2(guild_id) => lc_assertion_build::a2::build(&self.req, guild_id),

			Assertion::A3(guild_id, channel_id, role_id) =>
				lc_assertion_build::a3::build(&self.req, guild_id, channel_id, role_id),

			Assertion::A4(min_balance) => lc_assertion_build::a4::build(&self.req, min_balance),

			Assertion::A6 => lc_assertion_build::a6::build(&self.req),

			Assertion::A7(min_balance) => lc_assertion_build::a7::build(&self.req, min_balance),

			// no need to pass `networks` again because it's the same as the `get_supported_web3networks`
			Assertion::A8(_networks) => lc_assertion_build::a8::build(&self.req),

			Assertion::A10(min_balance) => lc_assertion_build::a10::build(&self.req, min_balance),

			Assertion::A11(min_balance) => lc_assertion_build::a11::build(&self.req, min_balance),

			Assertion::A13(owner) =>
				lc_assertion_build::a13::build(&self.req, self.context.ocall_api.clone(), &owner),

			Assertion::A14 => lc_assertion_build::a14::build(&self.req),

			Assertion::Achainable(param) => lc_assertion_build::achainable::build(&self.req, param),

			_ => {
				unimplemented!()
			},
		}?;

		// post-process the credential
		let signer = self.context.enclave_signer.as_ref();
		let enclave_account = signer.get_enclave_account().map_err(|e| {
			VCMPError::RequestVCFailed(
				self.req.assertion.clone(),
				ErrorDetail::StfError(ErrorString::truncate_from(format!("{e:?}").into())),
			)
		})?;

		let credential_endpoint =
			GLOBAL_DATA_PROVIDER_CONFIG.read().unwrap().credential_endpoint.clone();
		credential.credential_subject.set_endpoint(credential_endpoint);

		credential.issuer.id = DID::try_from(&Identity::Substrate(enclave_account.into()))
			.map_err(|e| {
				VCMPError::RequestVCFailed(
					self.req.assertion.clone(),
					ErrorDetail::StfError(ErrorString::truncate_from(format!("{e:?}").into())),
				)
			})?
			.format();
		let payload = credential.issuer.mrenclave.clone();
		let (enclave_account, sig) = signer.sign_vc_with_self(payload.as_bytes()).map_err(|e| {
			VCMPError::RequestVCFailed(
				self.req.assertion.clone(),
				ErrorDetail::StfError(ErrorString::truncate_from(format!("{e:?}").into())),
			)
		})?;
		debug!("Credential Payload signature: {:?}", sig);

		credential.add_proof(&sig, &enclave_account);
		credential.validate().map_err(|e| {
			VCMPError::RequestVCFailed(
				self.req.assertion.clone(),
				ErrorDetail::StfError(ErrorString::truncate_from(format!("{e:?}").into())),
			)
		})?;

		let vc_index = credential
			.get_index()
			.map_err(|e| {
				VCMPError::RequestVCFailed(
					self.req.assertion.clone(),
					ErrorDetail::StfError(ErrorString::truncate_from(format!("{e:?}").into())),
				)
			})?
			.into();
		let credential_str = credential.to_json().map_err(|_| {
			VCMPError::RequestVCFailed(self.req.assertion.clone(), ErrorDetail::ParseError)
		})?;
		debug!("Credential: {}, length: {}", credential_str, credential_str.len());
		let vc_hash = blake2_256(credential_str.as_bytes()).into();
		debug!("VC hash: {:?}", vc_hash);
		Ok((vc_index, vc_hash, credential_str.as_bytes().to_vec()))
	}

	fn on_success(&self, result: Self::Result) {
		debug!("Assertion build OK");
		// we shouldn't have the maximum text length limit in normal RSA3072 encryption, as the payload
		// using enclave's shielding key is encrypted in chunks
		let (vc_index, vc_hash, vc_payload) = result;
		if let Ok(enclave_signer) = self.context.enclave_signer.get_enclave_account() {
			let c = TrustedCall::request_vc_callback(
				enclave_signer.into(),
				self.req.who.clone(),
				self.req.assertion.clone(),
				vc_index,
				vc_hash,
				vc_payload,
				self.req.req_ext_hash,
			);
			let _ = self
				.context
				.submit_trusted_call(&self.req.shard, &self.req.top_hash, &c)
				.map_err(|e| error!("submit_trusted_call failed: {:?}", e));
		} else {
			error!("can't get enclave signer");
		}
	}

	fn on_failure(&self, error: Self::Error) {
		error!("Assertion build error: {error:?}");
		if let Ok(enclave_signer) = self.context.enclave_signer.get_enclave_account() {
			let c = TrustedCall::handle_vcmp_error(
				enclave_signer.into(),
				Some(self.req.who.clone()),
				error,
				self.req.req_ext_hash,
			);
			let _ = self
				.context
				.submit_trusted_call(&self.req.shard, &self.req.top_hash, &c)
				.map_err(|e| error!("submit_trusted_call failed: {:?}", e));
		} else {
			error!("can't get enclave signer");
		}
	}
}
