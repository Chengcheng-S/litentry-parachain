use codec::Error as CodecError;
use sgx_types::sgx_status_t;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("{0}")]
	Codec(#[from] CodecError),
	#[error("Enclave Error: {0}")]
	Sgx(sgx_status_t),
	#[error("Error, other: {0}")]
	Other(Box<dyn std::error::Error + Sync + Send + 'static>),
}
