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
	get_layer_two_nonce,
	trusted_cli::TrustedCli,
	trusted_command_utils::{get_accountid_from_str, get_identifiers, get_pair_from_str},
	trusted_operation::perform_trusted_operation,
	Cli, CliResult, CliResultOk,
};
use codec::Decode;
use ita_stf::{Index, TrustedCall, TrustedOperation};
use itp_stf_primitives::types::KeyPair;
use litentry_primitives::ParentchainBalance as Balance;
use log::*;
use sp_core::{crypto::Ss58Codec, Pair};
use std::boxed::Box;

#[derive(Parser)]
pub struct TransferCommand {
	/// sender's AccountId in ss58check format
	from: String,

	/// recipient's AccountId in ss58check format
	to: String,

	/// amount to be transferred
	amount: Balance,
}

impl TransferCommand {
	pub(crate) fn run(&self, cli: &Cli, trusted_args: &TrustedCli) -> CliResult {
		let from = get_pair_from_str(trusted_args, &self.from);
		let to = get_accountid_from_str(&self.to);
		info!("from ss58 is {}", from.public().to_ss58check());
		info!("to ss58 is {}", to.to_ss58check());

		let (mrenclave, shard) = get_identifiers(trusted_args);
		let nonce = get_layer_two_nonce!(from, cli, trusted_args);
		println!(
			"send trusted call transfer from {} to {}: {}, nonce: {}",
			from.public(),
			to,
			self.amount,
			nonce
		);
		let top: TrustedOperation =
			TrustedCall::balance_transfer(from.public().into(), to, self.amount)
				.sign(&KeyPair::Sr25519(Box::new(from)), nonce, &mrenclave, &shard)
				.into_trusted_operation(trusted_args.direct);
		let res = perform_trusted_operation(cli, trusted_args, &top).map(|_| CliResultOk::None)?;
		info!("trusted call transfer executed");
		Ok(res)
	}
}
