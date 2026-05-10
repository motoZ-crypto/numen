use crate as pallet_difficulty;
use frame_support::{derive_impl, traits::ConstU64};
use sp_core::U256;
use sp_runtime::BuildStorage;

#[frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Test>;

	#[runtime::pallet_index(1)]
	pub type Timestamp = pallet_timestamp::Pallet<Test>;

	#[runtime::pallet_index(2)]
	pub type Difficulty = pallet_difficulty::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlock<Test>;
}

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Test {}

impl pallet_difficulty::Config for Test {
	type TargetBlockTime = ConstU64<20>;
	type Halflife = ConstU64<1800>;
	type BreakThresholdSecs = ConstU64<1800>;
}

/// Initial difficulty used by tests.
pub const INITIAL_DIFFICULTY: u128 = 1_000_000;

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	let initial = U256::from(INITIAL_DIFFICULTY);
	pallet_difficulty::GenesisConfig::<Test> {
		initial_difficulty: initial,
		anchor_target: U256::MAX / initial,
		anchor_timestamp: 0,
		anchor_height: 0,
		_marker: Default::default(),
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	storage.into()
}

/// Advance to the given block, set the timestamp (in seconds),
/// and run `on_finalize` for both timestamp and difficulty pallets.
pub fn run_to_block_at(block: u64, now_secs: u64) -> u64  {
	use frame_support::traits::Hooks;
	System::set_block_number(block);
	let _ = pallet_timestamp::Pallet::<Test>::set(
		frame_system::RawOrigin::None.into(),
		now_secs * 1000,
	);
	<pallet_difficulty::Pallet<Test> as Hooks<u64>>::on_finalize(block);
	<pallet_timestamp ::Pallet<Test> as Hooks<u64>>::on_finalize(block);
	now_secs
}

/// Bring the chain past the auto-init block. Returns the timestamp used.
pub fn bootstrap(start_secs: u64) -> u64 {
	assert!(start_secs > 0, "bootstrap timestamp must be non-zero");
	run_to_block_at(1, start_secs)
}