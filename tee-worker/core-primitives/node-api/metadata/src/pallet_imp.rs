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

// TODO: maybe use macros to simplify this
use crate::{error::Result, NodeMetadata};

/// Pallet' name:
const IMP: &str = "IdentityManagement";

pub trait IMPCallIndexes {
	fn set_user_shielding_key_call_indexes(&self) -> Result<[u8; 2]>;
	fn link_identity_call_indexes(&self) -> Result<[u8; 2]>;
	fn deactivate_identity_call_indexes(&self) -> Result<[u8; 2]>;
	fn activate_identity_call_indexes(&self) -> Result<[u8; 2]>;
	fn user_shielding_key_set_call_indexes(&self) -> Result<[u8; 2]>;
	fn identity_linked_call_indexes(&self) -> Result<[u8; 2]>;
	fn identity_deactivated_call_indexes(&self) -> Result<[u8; 2]>;
	fn identity_activated_call_indexes(&self) -> Result<[u8; 2]>;
	fn imp_some_error_call_indexes(&self) -> Result<[u8; 2]>;
}

impl IMPCallIndexes for NodeMetadata {
	fn set_user_shielding_key_call_indexes(&self) -> Result<[u8; 2]> {
		self.call_indexes(IMP, "set_user_shielding_key")
	}

	fn link_identity_call_indexes(&self) -> Result<[u8; 2]> {
		self.call_indexes(IMP, "link_identity")
	}

	fn deactivate_identity_call_indexes(&self) -> Result<[u8; 2]> {
		self.call_indexes(IMP, "deactivate_identity")
	}

	fn activate_identity_call_indexes(&self) -> Result<[u8; 2]> {
		self.call_indexes(IMP, "activate_identity")
	}

	fn user_shielding_key_set_call_indexes(&self) -> Result<[u8; 2]> {
		self.call_indexes(IMP, "user_shielding_key_set")
	}

	fn identity_linked_call_indexes(&self) -> Result<[u8; 2]> {
		self.call_indexes(IMP, "identity_linked")
	}

	fn identity_deactivated_call_indexes(&self) -> Result<[u8; 2]> {
		self.call_indexes(IMP, "identity_deactivated")
	}

	fn identity_activated_call_indexes(&self) -> Result<[u8; 2]> {
		self.call_indexes(IMP, "identity_activated")
	}

	fn imp_some_error_call_indexes(&self) -> Result<[u8; 2]> {
		self.call_indexes(IMP, "some_error")
	}
}
