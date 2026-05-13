// Substrate and Polkadot dependencies
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU128, ConstU32, ConstU64, ConstU8, VariantCountOf},
	weights::{
		constants::{RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND},
		IdentityFee, Weight,
	},
};
use frame_system::limits::{BlockLength, BlockWeights};
use pallet_session::PeriodicSessions;
use pallet_transaction_payment::{FungibleAdapter, Multiplier, TargetedFeeAdjustment};
use sp_runtime::{traits::ConvertInto, FixedPointNumber, Perbill, Perquintill};
use sp_version::RuntimeVersion;

pub mod evm;

// Local module imports
use super::{
	AccountId, Balance, Balances, Block, BlockNumber, Hash, Nonce, PalletInfo, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeFreezeReason, RuntimeHoldReason, RuntimeOrigin, RuntimeTask,
	Session, SessionKeys, System, Validator, EXISTENTIAL_DEPOSIT, UNIT, VERSION,
	DAYS, MINUTES
};

pub(crate) const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
	pub const BlockHashCount: BlockNumber = 2400;
	pub const Version: RuntimeVersion = VERSION;

	pub const TargetBlockTime: u64 = 20;

	/// We allow for 2 seconds of compute with a 6 second average block time.
	pub RuntimeBlockWeights: BlockWeights = BlockWeights::with_sensible_defaults(
		Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
		NORMAL_DISPATCH_RATIO,
	);
	pub RuntimeBlockLength: BlockLength = BlockLength::builder()
		.max_length(5 * 1024 * 1024)
		.modify_max_length_for_class(
			frame_support::dispatch::DispatchClass::Normal,
			|max| *max = NORMAL_DISPATCH_RATIO * (5 * 1024 * 1024),
		)
		.build();
	pub const SS58Prefix: u8 = 42;
}

#[cfg(not(feature = "test-runtime"))]
parameter_types! {
	pub const DifficultyHalflife: u64 = 1800;
	pub const DifficultyBreakThresholdSecs: u64 = 1800;
}

#[cfg(feature = "test-runtime")]
parameter_types! {
	pub const DifficultyHalflife: u64 = 60;
	pub const DifficultyBreakThresholdSecs: u64 = 1800;
}

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`SoloChainDefaultConfig`](`struct@frame_system::config_preludes::SolochainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
	/// The block type for the runtime.
	type Block = Block;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = RuntimeBlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = RuntimeBlockLength;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = RocksDbWeight;
	/// Version of the runtime.
	type Version = Version;
	/// The data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_timestamp::Config for Runtime {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<10_000>;
	type WeightInfo = ();
}

impl pallet_balances::Config for Runtime {
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	/// The type for recording an account's balance.
	type Balance = Balance;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
	type AccountStore = System;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type DoneSlashHandler = ();
}

parameter_types! {
	/// Block reward: 50 UNIT per block to the miner.
	pub const BlockReward: Balance = 50 * super::UNIT;
}

impl pallet_reward::Config for Runtime {
	type Currency = Balances;
	type BlockReward = BlockReward;
}

parameter_types! {
	pub const TargetBlockFullness: Perquintill = Perquintill::from_percent(25);
	pub AdjustmentVariable: Multiplier = Multiplier::saturating_from_rational(3, 100_000);
	pub MinimumMultiplier: Multiplier = Multiplier::saturating_from_rational(1, 1_000_000u128);
	pub MaximumMultiplier: Multiplier = Multiplier::saturating_from_integer(10);
}

pub type SlowAdjustingFeeUpdate<R> = TargetedFeeAdjustment<
	R,
	TargetBlockFullness,
	AdjustmentVariable,
	MinimumMultiplier,
	MaximumMultiplier,
>;

impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = FungibleAdapter<Balances, ()>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = IdentityFee<Balance>;
	type LengthToFee = IdentityFee<Balance>;
	type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Runtime>;
	type WeightInfo = pallet_transaction_payment::weights::SubstrateWeight<Runtime>;
}

impl pallet_sudo::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

impl pallet_difficulty::Config for Runtime {
	type TargetBlockTime = TargetBlockTime;
	type Halflife = DifficultyHalflife;
	type BreakThresholdSecs = DifficultyBreakThresholdSecs;
}

parameter_types! {
	/// Maximum number of blocks a GRANDPA equivocation report transaction is
	/// considered valid for inclusion before expiring from the pool.
	pub const GrandpaReportLongevity: u64 = 25;
}

impl pallet_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type MaxAuthorities = ConstU32<1000>;
	type MaxNominators = ConstU32<0>;
	type MaxSetIdSessionEntries = ConstU64<168>;
	type KeyOwnerProof = sp_session::MembershipProof;
	type EquivocationReportSystem = pallet_grandpa::EquivocationReportSystem<
		Self,
		GrandpaOffenceReporter,
		pallet_session::historical::Pallet<Runtime>,
		GrandpaReportLongevity,
	>;
}

/// Block-author finder for `pallet-authorship`.
///
/// PoW does not have a first-class authority, so we do not attribute block
/// authorship for reward or reporter purposes here. The value is only consumed
/// by the GRANDPA equivocation report pipeline as a fallback reporter when an
/// offchain worker submits a report; leaving it `None` means the report carries
/// no reporter, which is fine because our reporting adapter ignores reporters.
pub struct PowFindAuthor;

impl frame_support::traits::FindAuthor<AccountId> for PowFindAuthor {
	fn find_author<'a, I>(_digests: I) -> Option<AccountId>
	where
		I: 'a + IntoIterator<Item = (sp_runtime::ConsensusEngineId, &'a [u8])>,
	{
		None
	}
}

impl pallet_authorship::Config for Runtime {
	type FindAuthor = PowFindAuthor;
	type EventHandler = ();
}

/// `pallet-session/historical` configuration.
///
/// Full identification is unit because this chain does not maintain exposures
/// or nominator stakes. The historical trie is required to validate GRANDPA
/// equivocation key-ownership proofs against the session in which the offence
/// occurred.
impl pallet_session::historical::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type FullIdentification = ();
	type FullIdentificationOf = UnitIdentification;
}

impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = AccountId;
	type ValidatorIdOf = ConvertInto;
	type ShouldEndSession = PeriodicSessions<SessionPeriod, SessionOffset>;
	type NextSessionRotation = PeriodicSessions<SessionPeriod, SessionOffset>;
	type SessionManager = pallet_session::historical::NoteHistoricalRoot<Runtime, Validator>;
	type SessionHandler = <SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
	type Keys = SessionKeys;
	type DisablingStrategy = ();
	type WeightInfo = pallet_session::weights::SubstrateWeight<Runtime>;
	type Currency = Balances;
	type KeyDeposit = ConstU128<0>;
}

parameter_types! {
	pub const ValidatorLockId: frame_support::traits::LockIdentifier = *b"validatr";
}

#[cfg(not(feature = "test-runtime"))]
parameter_types! {
	pub const SessionPeriod: BlockNumber = 10 * MINUTES;
	pub const SessionOffset: BlockNumber = 0;
}

#[cfg(not(feature = "test-runtime"))]
impl pallet_validator::Config for Runtime {
	type Currency = Balances;
	type SessionInterface = ValidatorSessionAdapter;
	type LockAmount = ConstU128<{ 1_000 * UNIT }>;
	#[allow(clippy::identity_op)]
	type LockDuration = ConstU32<{ 1 * DAYS }>;
	type LockId = ValidatorLockId;
	type MaxValidators = ConstU32<1_000>;
	#[allow(clippy::identity_op)]
	type RenewInterval = ConstU32<{ 1 * DAYS }>;
	type OfflineThreshold = ConstU32<1>;
	#[allow(clippy::identity_op)]
	type RejoinCooldownPeriod = ConstU32<{ 1 * DAYS }>;
}

#[cfg(feature = "test-runtime")]
parameter_types! {
	pub const SessionPeriod: BlockNumber = 3 * MINUTES;
	pub const SessionOffset: BlockNumber = 0 * MINUTES;
}

#[cfg(feature = "test-runtime")]
impl pallet_validator::Config for Runtime {
	type Currency = Balances;
	type SessionInterface = ValidatorSessionAdapter;
	type LockAmount = ConstU128<{ 1 * UNIT }>;
	type LockDuration = ConstU32<{ 20 * MINUTES }>;
	type LockId = ValidatorLockId;
	type MaxValidators = ConstU32<4>;
	type RenewInterval = ConstU32<{ 10 * MINUTES }>;
	type OfflineThreshold = ConstU32<1>;
	type RejoinCooldownPeriod = ConstU32<{ 1 * MINUTES }>;
}

/// Adapter wiring `pallet_validator::SessionInterface` to `pallet-session`.
/// Lets validator verify session keys exist before queuing a candidate.
pub struct ValidatorSessionAdapter;

impl pallet_validator::SessionInterface<AccountId> for ValidatorSessionAdapter {
	fn has_keys(who: &AccountId) -> bool {
		pallet_session::NextKeys::<Runtime>::contains_key(who)
	}
}
/// `ValidatorSetWithIdentification` adapter over `pallet-session`.
///
/// `pallet-im-online` requires its `ValidatorSet` to also expose an
/// `Identification` type so that offline reports can carry a per-validator
/// payload. We do not run a separate slashing/staking subsystem yet, so the
/// identification is a unit value.
pub struct UnitIdentification;

impl sp_runtime::traits::Convert<AccountId, Option<()>> for UnitIdentification {
	fn convert(_: AccountId) -> Option<()> {
		Some(())
	}
}

pub struct ValidatorIdentification;

impl frame_support::traits::ValidatorSet<AccountId> for ValidatorIdentification {
	type ValidatorId = <Session as frame_support::traits::ValidatorSet<AccountId>>::ValidatorId;
	type ValidatorIdOf = <Session as frame_support::traits::ValidatorSet<AccountId>>::ValidatorIdOf;

	fn session_index() -> sp_staking::SessionIndex {
		<Session as frame_support::traits::ValidatorSet<AccountId>>::session_index()
	}

	fn validators() -> alloc::vec::Vec<Self::ValidatorId> {
		<Session as frame_support::traits::ValidatorSet<AccountId>>::validators()
	}
}

impl frame_support::traits::ValidatorSetWithIdentification<AccountId> for ValidatorIdentification {
	type Identification = ();
	type IdentificationOf = UnitIdentification;
}


parameter_types! {
	/// Base priority for unsigned heartbeat extrinsics. Picked to be low enough
	/// not to crowd out other unsigned traffic but high enough to land within
	/// the session.
	pub const ImOnlineUnsignedPriority: sp_runtime::transaction_validity::TransactionPriority =
		sp_runtime::transaction_validity::TransactionPriority::MAX / 2;
	/// Maximum number of `ImOnlineId` keys stored per session.
	pub const ImOnlineMaxKeys: u32 = 1_000;
	/// Maximum peers reported in a single heartbeat payload.
	pub const ImOnlineMaxPeerInHeartbeats: u32 = 10_000;
}

pub struct ImOnlineOffenceReporter;

impl<O>
	sp_staking::offence::ReportOffence<AccountId, (AccountId, ()), O>
	for ImOnlineOffenceReporter
where
	O: sp_staking::offence::Offence<(AccountId, ())>,
{
	fn report_offence(
		_reporters: alloc::vec::Vec<AccountId>,
		offence: O,
	) -> Result<(), sp_staking::offence::OffenceError> {
		for (offender, _) in offence.offenders() {
			pallet_validator::Pallet::<Runtime>::note_offline(&offender);
		}
		Ok(())
	}

	fn is_known_offence(_offenders: &[(AccountId, ())], _time_slot: &O::TimeSlot) -> bool {
		false
	}
}

/// Offence reporter that forwards GRANDPA equivocation reports into
/// `pallet-validator`.
///
/// The `pallet-grandpa` built-in [`pallet_grandpa::EquivocationReportSystem`]
/// expects a [`ReportOffence`] implementation keyed by the
/// `IdentificationTuple` produced by `pallet-session/historical`, which in
/// this runtime is `(AccountId, ())`. Upon receiving a verified offence we
/// call [`pallet_validator::Pallet::note_equivocation`] for the offender,
/// which switches the validator's lock to `Kicked`, records the rejoin
/// cooldown deadline, and emits the `ValidatorKicked { Equivocation }`
/// event. The next session boundary removes the validator from the active
/// set via the existing session manager logic.
pub struct GrandpaOffenceReporter;

impl<O>
	sp_staking::offence::ReportOffence<AccountId, (AccountId, ()), O>
	for GrandpaOffenceReporter
where
	O: sp_staking::offence::Offence<(AccountId, ())>,
{
	fn report_offence(
		_reporters: alloc::vec::Vec<AccountId>,
		offence: O,
	) -> Result<(), sp_staking::offence::OffenceError> {
		for (offender, _) in offence.offenders() {
			pallet_validator::Pallet::<Runtime>::note_equivocation(&offender);
		}
		Ok(())
	}

	fn is_known_offence(_offenders: &[(AccountId, ())], _time_slot: &O::TimeSlot) -> bool {
		false
	}
}

impl pallet_im_online::Config for Runtime {
	type AuthorityId = pallet_im_online::sr25519::AuthorityId;
	type RuntimeEvent = RuntimeEvent;
	type ValidatorSet = ValidatorIdentification;
	type NextSessionRotation = PeriodicSessions<SessionPeriod, SessionOffset>;
	type ReportUnresponsiveness = ImOnlineOffenceReporter;
	type UnsignedPriority = ImOnlineUnsignedPriority;
	type MaxKeys = ImOnlineMaxKeys;
	type MaxPeerInHeartbeats = ImOnlineMaxPeerInHeartbeats;
	type WeightInfo = ();
}
