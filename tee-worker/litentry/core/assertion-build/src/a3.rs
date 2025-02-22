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

#[cfg(all(feature = "std", feature = "sgx"))]
compile_error!("feature \"std\" and feature \"sgx\" cannot be enabled at the same time");

#[cfg(all(not(feature = "std"), feature = "sgx"))]
extern crate sgx_tstd as std;

use crate::*;
use lc_data_providers::{discord_litentry::DiscordLitentryClient, vec_to_string};

const VC_A3_SUBJECT_DESCRIPTION: &str =
	"You have commented in Litentry Discord #🪂id-hubber channel. Channel link: https://discord.com/channels/807161594245152800/1093886939746291882";
const VC_A3_SUBJECT_TYPE: &str = "Active Discord ID-Hubber";

pub fn build(
	req: &AssertionBuildRequest,
	guild_id: ParameterString,
	channel_id: ParameterString,
	role_id: ParameterString,
) -> Result<Credential> {
	debug!("Assertion A3 build, who: {:?}", account_id_to_string(&req.who),);

	let mut has_commented: bool = false;

	let guild_id_s = vec_to_string(guild_id.to_vec()).map_err(|_| {
		Error::RequestVCFailed(
			Assertion::A3(guild_id.clone(), channel_id.clone(), role_id.clone()),
			ErrorDetail::ParseError,
		)
	})?;
	let channel_id_s = vec_to_string(channel_id.to_vec()).map_err(|_| {
		Error::RequestVCFailed(
			Assertion::A3(guild_id.clone(), channel_id.clone(), role_id.clone()),
			ErrorDetail::ParseError,
		)
	})?;
	let role_id_s = vec_to_string(role_id.to_vec()).map_err(|_| {
		Error::RequestVCFailed(
			Assertion::A3(guild_id.clone(), channel_id.clone(), role_id.clone()),
			ErrorDetail::ParseError,
		)
	})?;

	let mut client = DiscordLitentryClient::new();
	for identity in &req.identities {
		if let Identity::Discord(address) = &identity.0 {
			let resp = client
				.check_id_hubber(
					guild_id.to_vec(),
					channel_id.to_vec(),
					role_id.to_vec(),
					address.to_vec(),
				)
				.map_err(|e| {
					Error::RequestVCFailed(
						Assertion::A3(guild_id.clone(), channel_id.clone(), role_id.clone()),
						e.into_error_detail(),
					)
				})?;

			if resp.data {
				has_commented = true;
				break
			}
		}
	}

	match Credential::new(&req.who, &req.shard) {
		Ok(mut credential_unsigned) => {
			credential_unsigned.add_subject_info(VC_A3_SUBJECT_DESCRIPTION, VC_A3_SUBJECT_TYPE);
			credential_unsigned.add_assertion_a3(
				has_commented,
				guild_id_s,
				channel_id_s,
				role_id_s,
			);
			Ok(credential_unsigned)
		},
		Err(e) => {
			error!("Generate unsigned credential A3 failed {:?}", e);
			Err(Error::RequestVCFailed(
				Assertion::A3(guild_id, channel_id, role_id),
				e.into_error_detail(),
			))
		},
	}
}

#[cfg(test)]
mod tests {
	use crate::{a3::build, AccountId, AssertionBuildRequest};
	use frame_support::BoundedVec;
	use itp_stf_primitives::types::ShardIdentifier;
	use lc_data_providers::GLOBAL_DATA_PROVIDER_CONFIG;
	use litentry_primitives::{Assertion, Identity, IdentityNetworkTuple, IdentityString};
	use log;
	use std::{format, vec, vec::Vec};

	#[test]
	fn build_a3_works() {
		GLOBAL_DATA_PROVIDER_CONFIG
			.write()
			.unwrap()
			.set_discord_litentry_url("http://localhost:19527".to_string());
		let guild_id_u: u64 = 919848390156767232;
		let channel_id_u: u64 = 919848392035794945;
		let role_id_u: u64 = 1034083718425493544;

		let guild_id_vec: Vec<u8> = format!("{}", guild_id_u).as_bytes().to_vec();
		let channel_id_vec: Vec<u8> = format!("{}", channel_id_u).as_bytes().to_vec();
		let role_id_vec: Vec<u8> = format!("{}", role_id_u).as_bytes().to_vec();

		let handler_vec: Vec<u8> = "againstwar".to_string().as_bytes().to_vec();
		let identities: Vec<IdentityNetworkTuple> =
			vec![(Identity::Discord(IdentityString::truncate_from(handler_vec)), vec![])];

		let guild_id = BoundedVec::try_from(guild_id_vec).unwrap();
		let channel_id = BoundedVec::try_from(channel_id_vec).unwrap();
		let role_id = BoundedVec::try_from(role_id_vec).unwrap();

		let req = AssertionBuildRequest {
			shard: ShardIdentifier::default(),
			signer: AccountId::from([0; 32]),
			enclave_account: AccountId::from([0; 32]),
			who: AccountId::from([0; 32]).into(),
			assertion: Assertion::A3(guild_id.clone(), channel_id.clone(), role_id.clone()),
			identities,
			top_hash: Default::default(),
			req_ext_hash: Default::default(),
		};

		let _ = build(&req, guild_id, channel_id, role_id);
		log::info!("build A3 done");
	}
}
