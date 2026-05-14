//! Test runtime + `PrecompileSet` wiring for the balances-erc20 precompile.
//!
//! Uses `precompile_utils::testing::PrecompilesTester` (via `prepare_test`)
//! to drive the precompile, mirroring the test harness layout used by
//! Moonbeam.

use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{ConstU128, ConstU64, FindAuthor},
	weights::Weight,
};
use pallet_evm::{
	EnsureAddressNever, EnsureAddressRoot, FrameSystemAccountProvider, HashedAddressMapping,
	SubstrateBlockHashMapping,
};
use precompile_utils::precompile_set::{AddressU64, PrecompileAt, PrecompileSetBuilder};
use sp_core::{H160, U256};
use sp_runtime::{traits::BlakeTwo256, AccountId32, BuildStorage};

use crate::{Erc20BalancesPrecompile, Erc20Metadata};

pub type AccountId = AccountId32;
pub type Balance = u128;
pub type Block = frame_system::mocking::MockBlock<Runtime>;

/// Test metadata mirroring the runtime's native UNIT token.
pub struct NativeErc20Metadata;
impl Erc20Metadata for NativeErc20Metadata {
	const NAME: &'static str = "UNIT";
	const SYMBOL: &'static str = "UNIT";
	const DECIMALS: u8 = 18;
}

/// Auto-generated PCall enum (one variant per `#[precompile::public]`
/// method) makes type-safe call construction in tests trivial.
pub type PCall = crate::Erc20BalancesPrecompileCall<Runtime, NativeErc20Metadata>;

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Timestamp: pallet_timestamp,
		Balances: pallet_balances,
		Evm: pallet_evm,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type AccountId = AccountId;
	type AccountData = pallet_balances::AccountData<Balance>;
	type Lookup = sp_runtime::traits::IdentityLookup<AccountId>;
}

parameter_types! {
	pub const MinimumPeriod: u64 = 1;
}

impl pallet_timestamp::Config for Runtime {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = ();
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = System;
}

const MAX_POV_SIZE: u64 = 5 * 1024 * 1024;
const BLOCK_STORAGE_LIMIT: u64 = 40 * 1024;

parameter_types! {
	pub BlockGasLimit: U256 = U256::from(u64::MAX);
	pub const WeightPerGas: Weight = Weight::from_parts(20_000, 0);
	pub GasLimitPovSizeRatio: u64 = {
		let l = BlockGasLimit::get().min(u64::MAX.into()).low_u64();
		l.saturating_div(MAX_POV_SIZE)
	};
	pub GasLimitStorageGrowthRatio: u64 = {
		let l = BlockGasLimit::get().min(u64::MAX.into()).low_u64();
		l.saturating_div(BLOCK_STORAGE_LIMIT)
	};
	pub PrecompilesValue: Precompiles<Runtime> = Precompiles::new();
}

/// `FindAuthor` returns the zero address — block-author irrelevant for the
/// precompile under test.
pub struct AuthorZero;
impl FindAuthor<H160> for AuthorZero {
	fn find_author<'a, I>(_d: I) -> Option<H160>
	where
		I: 'a + IntoIterator<Item = (sp_runtime::ConsensusEngineId, &'a [u8])>,
	{
		Some(H160::zero())
	}
}

/// Address recorded as `code_address` and `address` in the EVM context.
/// Matches the runtime's deployment slot (`0x0000000000000000000000000000000000000802`).
pub const PRECOMPILE_ADDRESS: H160 = H160(hex_literal::hex!(
	"0000000000000000000000000000000000000802"
));

/// Numeric form of [`PRECOMPILE_ADDRESS`] used by `AddressU64<2050>`.
pub const PRECOMPILE_ADDR_U64: u64 = 0x802;

/// Concrete instantiation under test.
pub type Precompiles<R> = PrecompileSetBuilder<
	R,
	(PrecompileAt<AddressU64<PRECOMPILE_ADDR_U64>, Erc20BalancesPrecompile<R, NativeErc20Metadata>>,),
>;

/// New PrecompileSet instance for use with `prepare_test`.
pub fn precompiles() -> Precompiles<Runtime> {
	PrecompilesValue::get()
}

impl pallet_evm::Config for Runtime {
	type AccountProvider = FrameSystemAccountProvider<Self>;
	type FeeCalculator = ();
	type GasWeightMapping = pallet_evm::FixedGasWeightMapping<Self>;
	type WeightPerGas = WeightPerGas;
	type BlockHashMapping = SubstrateBlockHashMapping<Self>;
	type CallOrigin = EnsureAddressRoot<AccountId>;
	type CreateOriginFilter = ();
	type CreateInnerOriginFilter = ();
	type WithdrawOrigin = EnsureAddressNever<AccountId>;
	type AddressMapping = HashedAddressMapping<BlakeTwo256>;
	type Currency = Balances;
	type PrecompilesType = Precompiles<Self>;
	type PrecompilesValue = PrecompilesValue;
	type ChainId = ConstU64<1>;
	type BlockGasLimit = BlockGasLimit;
	type TransactionGasLimit = ();
	type Runner = pallet_evm::runner::stack::Runner<Self>;
	type OnChargeTransaction = ();
	type OnCreate = ();
	type FindAuthor = AuthorZero;
	type GasLimitPovSizeRatio = GasLimitPovSizeRatio;
	type GasLimitStorageGrowthRatio = GasLimitStorageGrowthRatio;
	type Timestamp = Timestamp;
	type WeightInfo = ();
}

// -- ExtBuilder ------------------------------------------------------------

#[derive(Default)]
pub struct ExtBuilder {
	balances: alloc::vec::Vec<(AccountId, Balance)>,
}

impl ExtBuilder {
	pub fn with_balances(mut self, b: alloc::vec::Vec<(AccountId, Balance)>) -> Self {
		self.balances = b;
		self
	}

	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Runtime>::default()
			.build_storage()
			.expect("default genesis builds");
		pallet_balances::GenesisConfig::<Runtime> {
			balances: self.balances,
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.expect("balances genesis assimilates");
		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}

/// Mirror account derived from an EVM caller via the runtime's
/// `AddressMapping`.
pub fn mirror(addr: H160) -> AccountId {
	use pallet_evm::AddressMapping;
	HashedAddressMapping::<BlakeTwo256>::into_account_id(addr)
}

/// Convenience H160 generator: low byte = `i`.
pub fn h160(i: u8) -> H160 {
	let mut bytes = [0u8; 20];
	bytes[19] = i;
	H160(bytes)
}

/// Convenience AccountId32 generator: every byte = `i`.
pub fn aid(i: u8) -> AccountId {
	AccountId32::from([i; 32])
}
