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

#[cfg(all(not(feature = "std"), feature = "sgx"))]
use crate::sgx_reexport_prelude::*;
pub use litentry_primitives::{ErrorDetail, IMPError, VCMPError};

use lc_scheduled_enclave::error::Error as ScheduledEnclaveError;
use sgx_types::sgx_status_t;
use sp_runtime::traits::LookupError;
use std::{boxed::Box, format};

pub type Result<T> = core::result::Result<T, Error>;

/// Indirect calls execution error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("SGX error, status: {0}")]
	Sgx(sgx_status_t),
	#[error("STF execution error: {0}")]
	StfExecution(#[from] itp_stf_executor::error::Error),
	#[error("Node Metadata error: {0:?}")]
	NodeMetadata(itp_node_api::metadata::Error),
	#[error("Node metadata provider error: {0:?}")]
	NodeMetadataProvider(#[from] itp_node_api::metadata::provider::Error),
	#[error("Crypto error: {0}")]
	Crypto(itp_sgx_crypto::Error),
	#[error(transparent)]
	Other(#[from] Box<dyn std::error::Error + Sync + Send + 'static>),
	#[error("AccountId lookup error")]
	AccountIdLookup,
	#[error("convert parent chain block number error")]
	ConvertParentchainBlockNumber,
	#[error("IMP handling error: {0:?}")]
	IMPHandlingError(IMPError),
	#[error("VCMP handling error: {0:?}")]
	VCMPHandlingError(VCMPError),
	#[error("BatchAll handling error")]
	BatchAllHandlingError,
	#[error("ScheduledEnclave Error: {0:?}")]
	ImportScheduledEnclave(ScheduledEnclaveError),
}

impl From<sgx_status_t> for Error {
	fn from(sgx_status: sgx_status_t) -> Self {
		Self::Sgx(sgx_status)
	}
}

impl From<itp_sgx_crypto::Error> for Error {
	fn from(e: itp_sgx_crypto::Error) -> Self {
		Self::Crypto(e)
	}
}

impl From<codec::Error> for Error {
	fn from(e: codec::Error) -> Self {
		Self::Other(format!("{:?}", e).into())
	}
}

impl From<itp_node_api::metadata::Error> for Error {
	fn from(e: itp_node_api::metadata::Error) -> Self {
		Self::NodeMetadata(e)
	}
}

impl From<LookupError> for Error {
	fn from(_: LookupError) -> Self {
		Self::AccountIdLookup
	}
}

impl From<IMPError> for Error {
	fn from(e: IMPError) -> Self {
		Self::IMPHandlingError(e)
	}
}

impl From<VCMPError> for Error {
	fn from(e: VCMPError) -> Self {
		Self::VCMPHandlingError(e)
	}
}

impl From<ScheduledEnclaveError> for Error {
	fn from(e: ScheduledEnclaveError) -> Self {
		Self::ImportScheduledEnclave(e)
	}
}
