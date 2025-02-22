// Auto-generated via `yarn polkadot-types-from-chain`, do not edit
/* eslint-disable */

// import type lookup before we augment - in some environments
// this is required to allow for ambient/previous definitions
import "@polkadot/api-base/types/storage";

import type { ApiTypes, AugmentedQuery, QueryableStorageEntry } from "@polkadot/api-base/types";
import type {
    Bytes,
    Null,
    Option,
    U8aFixed,
    Vec,
    bool,
    u128,
    u32,
    u64,
} from "@polkadot/types-codec";
import type { AnyNumber, ITuple } from "@polkadot/types-codec/types";
import type { AccountId32, H256 } from "@polkadot/types/interfaces/runtime";
import type {
    FrameSupportDispatchPerDispatchClassWeight,
    FrameSystemAccountInfo,
    FrameSystemEventRecord,
    FrameSystemLastRuntimeUpgradeInfo,
    FrameSystemPhase,
    LitentryPrimitivesIdentity,
    PalletBalancesAccountData,
    PalletBalancesBalanceLock,
    PalletBalancesReserveData,
    PalletIdentityManagementTeeIdentityContext,
    PalletTransactionPaymentReleases,
    SpRuntimeDigest,
} from "@polkadot/types/lookup";
import type { Observable } from "@polkadot/types/types";

export type __AugmentedQuery<ApiType extends ApiTypes> = AugmentedQuery<ApiType, () => unknown>;
export type __QueryableStorageEntry<ApiType extends ApiTypes> = QueryableStorageEntry<ApiType>;

declare module "@polkadot/api-base/types/storage" {
    interface AugmentedQueries<ApiType extends ApiTypes> {
        balances: {
            /**
             * The Balances pallet example of storing the balance of an account.
             *
             * # Example
             *
             * ```nocompile
             * impl pallet_balances::Config for Runtime {
             * type AccountStore = StorageMapShim<Self::Account<Runtime>, frame_system::Provider<Runtime>, AccountId, Self::AccountData<Balance>>
             * }
             * ```
             *
             * You can also store the balance of an account in the `System` pallet.
             *
             * # Example
             *
             * ```nocompile
             * impl pallet_balances::Config for Runtime {
             * type AccountStore = System
             * }
             * ```
             *
             * But this comes with tradeoffs, storing account balances in the system pallet stores
             * `frame_system` data alongside the account data contrary to storing account balances in the
             * `Balances` pallet, which uses a `StorageMap` to store balances data only.
             * NOTE: This is only used in the case that this pallet is used to store balances.
             **/
            account: AugmentedQuery<
                ApiType,
                (arg: AccountId32 | string | Uint8Array) => Observable<PalletBalancesAccountData>,
                [AccountId32]
            >;
            /**
             * The total units of outstanding deactivated balance in the system.
             **/
            inactiveIssuance: AugmentedQuery<ApiType, () => Observable<u128>, []>;
            /**
             * Any liquidity locks on some account balances.
             * NOTE: Should only be accessed when setting, changing and freeing a lock.
             **/
            locks: AugmentedQuery<
                ApiType,
                (
                    arg: AccountId32 | string | Uint8Array
                ) => Observable<Vec<PalletBalancesBalanceLock>>,
                [AccountId32]
            >;
            /**
             * Named reserves on some account balances.
             **/
            reserves: AugmentedQuery<
                ApiType,
                (
                    arg: AccountId32 | string | Uint8Array
                ) => Observable<Vec<PalletBalancesReserveData>>,
                [AccountId32]
            >;
            /**
             * The total units issued in the system.
             **/
            totalIssuance: AugmentedQuery<ApiType, () => Observable<u128>, []>;
        };
        identityManagement: {
            idGraphLens: AugmentedQuery<
                ApiType,
                (
                    arg:
                        | LitentryPrimitivesIdentity
                        | { Twitter: any }
                        | { Discord: any }
                        | { Github: any }
                        | { Substrate: any }
                        | { Evm: any }
                        | string
                        | Uint8Array
                ) => Observable<u32>,
                [LitentryPrimitivesIdentity]
            >;
            idGraphs: AugmentedQuery<
                ApiType,
                (
                    arg1:
                        | LitentryPrimitivesIdentity
                        | { Twitter: any }
                        | { Discord: any }
                        | { Github: any }
                        | { Substrate: any }
                        | { Evm: any }
                        | string
                        | Uint8Array,
                    arg2:
                        | LitentryPrimitivesIdentity
                        | { Twitter: any }
                        | { Discord: any }
                        | { Github: any }
                        | { Substrate: any }
                        | { Evm: any }
                        | string
                        | Uint8Array
                ) => Observable<Option<PalletIdentityManagementTeeIdentityContext>>,
                [LitentryPrimitivesIdentity, LitentryPrimitivesIdentity]
            >;
            linkedIdentities: AugmentedQuery<
                ApiType,
                (
                    arg:
                        | LitentryPrimitivesIdentity
                        | { Twitter: any }
                        | { Discord: any }
                        | { Github: any }
                        | { Substrate: any }
                        | { Evm: any }
                        | string
                        | Uint8Array
                ) => Observable<Option<Null>>,
                [LitentryPrimitivesIdentity]
            >;
            userShieldingKeys: AugmentedQuery<
                ApiType,
                (
                    arg:
                        | LitentryPrimitivesIdentity
                        | { Twitter: any }
                        | { Discord: any }
                        | { Github: any }
                        | { Substrate: any }
                        | { Evm: any }
                        | string
                        | Uint8Array
                ) => Observable<Option<U8aFixed>>,
                [LitentryPrimitivesIdentity]
            >;
        };
        parentchain: {
            /**
             * Hash of the last block. Set by `set_block`.
             **/
            blockHash: AugmentedQuery<ApiType, () => Observable<H256>, []>;
            /**
             * The current block number being processed. Set by `set_block`.
             **/
            number: AugmentedQuery<ApiType, () => Observable<u32>, []>;
            /**
             * Hash of the previous block. Set by `set_block`.
             **/
            parentHash: AugmentedQuery<ApiType, () => Observable<H256>, []>;
        };
        sudo: {
            /**
             * The `AccountId` of the sudo key.
             **/
            key: AugmentedQuery<ApiType, () => Observable<Option<AccountId32>>, []>;
        };
        system: {
            /**
             * The full account information for a particular account ID.
             **/
            account: AugmentedQuery<
                ApiType,
                (arg: AccountId32 | string | Uint8Array) => Observable<FrameSystemAccountInfo>,
                [AccountId32]
            >;
            /**
             * Total length (in bytes) for all extrinsics put together, for the current block.
             **/
            allExtrinsicsLen: AugmentedQuery<ApiType, () => Observable<Option<u32>>, []>;
            /**
             * Map of block numbers to block hashes.
             **/
            blockHash: AugmentedQuery<
                ApiType,
                (arg: u32 | AnyNumber | Uint8Array) => Observable<H256>,
                [u32]
            >;
            /**
             * The current weight for the block.
             **/
            blockWeight: AugmentedQuery<
                ApiType,
                () => Observable<FrameSupportDispatchPerDispatchClassWeight>,
                []
            >;
            /**
             * Digest of the current block, also part of the block header.
             **/
            digest: AugmentedQuery<ApiType, () => Observable<SpRuntimeDigest>, []>;
            /**
             * The number of events in the `Events<T>` list.
             **/
            eventCount: AugmentedQuery<ApiType, () => Observable<u32>, []>;
            /**
             * Events deposited for the current block.
             *
             * NOTE: The item is unbound and should therefore never be read on chain.
             * It could otherwise inflate the PoV size of a block.
             *
             * Events have a large in-memory size. Box the events to not go out-of-memory
             * just in case someone still reads them from within the runtime.
             **/
            events: AugmentedQuery<ApiType, () => Observable<Vec<FrameSystemEventRecord>>, []>;
            /**
             * Mapping between a topic (represented by T::Hash) and a vector of indexes
             * of events in the `<Events<T>>` list.
             *
             * All topic vectors have deterministic storage locations depending on the topic. This
             * allows light-clients to leverage the changes trie storage tracking mechanism and
             * in case of changes fetch the list of events of interest.
             *
             * The value has the type `(T::BlockNumber, EventIndex)` because if we used only just
             * the `EventIndex` then in case if the topic has the same contents on the next block
             * no notification will be triggered thus the event might be lost.
             **/
            eventTopics: AugmentedQuery<
                ApiType,
                (arg: H256 | string | Uint8Array) => Observable<Vec<ITuple<[u32, u32]>>>,
                [H256]
            >;
            /**
             * The execution phase of the block.
             **/
            executionPhase: AugmentedQuery<ApiType, () => Observable<Option<FrameSystemPhase>>, []>;
            /**
             * Total extrinsics count for the current block.
             **/
            extrinsicCount: AugmentedQuery<ApiType, () => Observable<Option<u32>>, []>;
            /**
             * Extrinsics data for the current block (maps an extrinsic's index to its data).
             **/
            extrinsicData: AugmentedQuery<
                ApiType,
                (arg: u32 | AnyNumber | Uint8Array) => Observable<Bytes>,
                [u32]
            >;
            /**
             * Stores the `spec_version` and `spec_name` of when the last runtime upgrade happened.
             **/
            lastRuntimeUpgrade: AugmentedQuery<
                ApiType,
                () => Observable<Option<FrameSystemLastRuntimeUpgradeInfo>>,
                []
            >;
            /**
             * The current block number being processed. Set by `execute_block`.
             **/
            number: AugmentedQuery<ApiType, () => Observable<u32>, []>;
            /**
             * Hash of the previous block.
             **/
            parentHash: AugmentedQuery<ApiType, () => Observable<H256>, []>;
            /**
             * True if we have upgraded so that AccountInfo contains three types of `RefCount`. False
             * (default) if not.
             **/
            upgradedToTripleRefCount: AugmentedQuery<ApiType, () => Observable<bool>, []>;
            /**
             * True if we have upgraded so that `type RefCount` is `u32`. False (default) if not.
             **/
            upgradedToU32RefCount: AugmentedQuery<ApiType, () => Observable<bool>, []>;
        };
        timestamp: {
            /**
             * Did the timestamp get updated in this block?
             **/
            didUpdate: AugmentedQuery<ApiType, () => Observable<bool>, []>;
            /**
             * Current time for the current block.
             **/
            now: AugmentedQuery<ApiType, () => Observable<u64>, []>;
        };
        transactionPayment: {
            nextFeeMultiplier: AugmentedQuery<ApiType, () => Observable<u128>, []>;
            storageVersion: AugmentedQuery<
                ApiType,
                () => Observable<PalletTransactionPaymentReleases>,
                []
            >;
        };
    } // AugmentedQueries
} // declare module
