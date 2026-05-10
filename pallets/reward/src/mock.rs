use crate as pallet_reward;
use codec::Encode;
use frame_support::{derive_impl, parameter_types, traits::ConstU128};
use sp_consensus_pow::POW_ENGINE_ID;
use sp_runtime::{AccountId32, BuildStorage, DigestItem, traits::IdentityLookup};

pub type Balance = u128;

type Block = frame_system::mocking::MockBlock<Test>;

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
		RuntimeTask
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Test>;

	#[runtime::pallet_index(1)]
	pub type Balances = pallet_balances::Pallet<Test>;

	#[runtime::pallet_index(2)]
	pub type BlockReward = pallet_reward::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId32;
	type Lookup = IdentityLookup<AccountId32>;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
	type Balance = Balance;
	type ExistentialDeposit = ConstU128<1>;
}

parameter_types! {
	pub const Reward: Balance = 50_000_000_000_000_000_000;
}

impl pallet_reward::Config for Test {
	type Currency = Balances;
	type BlockReward = Reward;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap();
	storage.into()
}

pub fn set_author_digest(author: &AccountId32) {
	let digest_item = DigestItem::PreRuntime(POW_ENGINE_ID, author.encode());
	frame_system::Pallet::<Test>::deposit_log(digest_item);
}