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

/// Here's an example of different assertions in this VC:
///
/// Imagine:
/// ALICE holds 100 LITs since 2018-03-02
/// BOB   holds 100 LITs since 2023-03-07
/// CAROL holds 0.1 LITs since 2020-02-22
///
/// min_amount is 1 LIT
///
/// If they all request A4, these are the received assertions:
/// ALICE:
/// [
///    from_date: < 2019-01-01
///    to_date: >= 2023-03-30 (now)
///    value: true
/// ]
///
/// BOB:
/// [
///    from_date: < 2017-01-01
///    to_date: >= 2023-03-30 (now)
///    value: false
/// ]
///
/// CAROL:
/// [
///    from_date: < 2017-01-01
///    to_date: >= 2023-03-30 (now)
///    value: false
/// ]
///
/// So just from the assertion results you can't distinguish between:
/// BOB, who just started to hold recently,
/// and CAROL, who has been holding for 3 years, but with too little amount
///
/// This is because the data provider doesn't provide more information, it only
/// takes the query with from_date and min_ammount, and returns true or false.
///
/// Please note:
/// the operators are mainly for IDHub's parsing, we will **NEVER** have:
/// - `from_date` with >= op, nor
/// - `value` is false but the `from_date` is something other than 2017-01-01.
///  
use crate::*;
use lc_data_providers::{
	achainable::{AchainableClient, AchainableHolder, ParamsBasicTypeWithAmountHolding},
	vec_to_string, LIT_TOKEN_ADDRESS,
};
use std::string::ToString;

const VC_A4_SUBJECT_DESCRIPTION: &str =
	"The length of time a user continues to hold a particular token (with particular threshold of token amount)";
const VC_A4_SUBJECT_TYPE: &str = "LIT Holding Time";

pub fn build(req: &AssertionBuildRequest, min_balance: ParameterString) -> Result<Credential> {
	debug!("Assertion A4 build, who: {:?}", account_id_to_string(&req.who));

	let q_min_balance = vec_to_string(min_balance.to_vec()).map_err(|_| {
		Error::RequestVCFailed(Assertion::A4(min_balance.clone()), ErrorDetail::ParseError)
	})?;

	let mut client = AchainableClient::new();
	let identities = transpose_identity(&req.identities);

	let mut is_hold = false;
	let mut optimal_hold_index = usize::MAX;

	// If both Substrate and Evm networks meet the conditions, take the interval with the longest holding time.
	// Here's an example:
	//
	// ALICE holds 100 LITs since 2018-03-02 on substrate network
	// ALICE holds 100 LITs since 2020-03-02 on evm network
	//
	// min_amount is 1 LIT
	//
	// the result should be
	// Alice:
	// [
	//    from_date: < 2019-01-01
	//    to_date: >= 2023-03-30 (now)
	//    value: true
	// ]
	for (network, addresses) in identities {
		// If found query result is the optimal solution, i.e optimal_hold_index = 0, (2017-01-01)
		// there is no need to query other networks.
		if optimal_hold_index == 0 {
			break
		}

		let token =
			if network == Web3Network::Ethereum { Some(LIT_TOKEN_ADDRESS.into()) } else { None };

		let addresses: Vec<String> = addresses.into_iter().collect();
		for (index, date) in ASSERTION_FROM_DATE.iter().enumerate() {
			for address in &addresses {
				let holding = ParamsBasicTypeWithAmountHolding::new(
					&network,
					q_min_balance.to_string(),
					date.to_string(),
					token.clone(),
				);
				let is_amount_holder = client.is_holder(address, holding).map_err(|e| {
					error!("Assertion A4 request is_holder error: {:?}", e);
					Error::RequestVCFailed(
						Assertion::A4(min_balance.clone()),
						e.into_error_detail(),
					)
				})?;

				if is_amount_holder {
					if index < optimal_hold_index {
						optimal_hold_index = index;
					}

					is_hold = true;

					break
				}
			}
		}

		// TODO:
		// There's an issue for this: https://github.com/litentry/litentry-parachain/issues/1655
		//
		// There is a problem here, because TDF does not support mixed network types,
		// It is need to request TDF 2 (substrate+evm networks) * 14 (ASSERTION_FROM_DATE) * addresses http requests.
		// If TDF can handle mixed network type, and even supports from_date array,
		// so that ideally, up to one http request can yield results.
	}

	// If is_hold is false, then the optimal_hold_index is always 0 (2017-01-01)
	if !is_hold {
		optimal_hold_index = 0;
	}

	match Credential::new(&req.who, &req.shard) {
		Ok(mut credential_unsigned) => {
			credential_unsigned.add_subject_info(VC_A4_SUBJECT_DESCRIPTION, VC_A4_SUBJECT_TYPE);
			credential_unsigned.update_holder(
				is_hold,
				&q_min_balance,
				&ASSERTION_FROM_DATE[optimal_hold_index].into(),
			);

			Ok(credential_unsigned)
		},
		Err(e) => {
			error!("Generate unsigned credential failed {:?}", e);
			Err(Error::RequestVCFailed(Assertion::A4(min_balance), e.into_error_detail()))
		},
	}
}
