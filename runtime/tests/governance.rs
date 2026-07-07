//! Governance wiring. Covers the genesis treasury endowment plus the bounty and
//! child-bounty lifecycles settled under a root approval origin.

use frame_support::{
	assert_ok,
	traits::{tokens::fungible::Mutate, OnInitialize},
};
use solochain_template_runtime::{
	configs, genesis_config_presets, AccountId, Balances, Bounties, ChildBounties, Runtime,
	RuntimeOrigin, System, Treasury, UNIT,
};
use sp_io::TestExternalities;
use sp_keyring::Sr25519Keyring;
use sp_runtime::{traits::StaticLookup, BuildStorage};

type LookupSource = <<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source;

fn acc(who: Sr25519Keyring) -> AccountId {
	who.to_account_id()
}

fn src(who: &AccountId) -> LookupSource {
	<Runtime as frame_system::Config>::Lookup::unlookup(who.clone())
}

fn new_test_ext() -> TestExternalities {
	let storage = frame_system::GenesisConfig::<Runtime>::default()
		.build_storage()
		.expect("system genesis builds");
	let mut ext = TestExternalities::from(storage);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// Drive the treasury to its next spend boundary so an approved bounty gets
/// funded from the pot.
fn fund_approved_bounties() {
	let spend_period = <Runtime as pallet_treasury::Config>::SpendPeriod::get();
	System::set_block_number(spend_period);
	Treasury::on_initialize(spend_period);
}

/// Take a parent bounty from proposal to an accepted curator, returning its id.
fn active_parent_bounty(proposer: &AccountId, curator: &AccountId, value: u128, fee: u128) -> u32 {
	assert_ok!(Bounties::propose_bounty(
		RuntimeOrigin::signed(proposer.clone()),
		value,
		b"parent bounty".to_vec(),
	));
	let id = 0;
	assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), id));
	fund_approved_bounties();
	assert_ok!(Bounties::propose_curator(RuntimeOrigin::root(), id, src(curator), fee));
	assert_ok!(Bounties::accept_curator(RuntimeOrigin::signed(curator.clone()), id));
	id
}

#[test]
fn bounty_pays_beneficiary_after_root_approval() {
	let proposer = acc(Sr25519Keyring::Alice);
	let curator = acc(Sr25519Keyring::Bob);
	let beneficiary = acc(Sr25519Keyring::Charlie);
	let treasury = configs::TreasuryAccount::get();

	new_test_ext().execute_with(|| {
		Balances::set_balance(&proposer, 10_000 * UNIT);
		Balances::set_balance(&curator, 10_000 * UNIT);
		Balances::set_balance(&treasury, 1_000_000 * UNIT);

		let value = 1_000 * UNIT;
		let fee = 100 * UNIT;
		let id = active_parent_bounty(&proposer, &curator, value, fee);

		assert_ok!(Bounties::award_bounty(
			RuntimeOrigin::signed(curator.clone()),
			id,
			src(&beneficiary),
		));

		let delay = <Runtime as pallet_bounties::Config>::BountyDepositPayoutDelay::get();
		System::set_block_number(System::block_number() + delay + 1);
		assert_ok!(Bounties::claim_bounty(RuntimeOrigin::signed(proposer), id));

		assert_eq!(Balances::free_balance(&beneficiary), value - fee);
	});
}

#[test]
fn child_bounty_pays_beneficiary_without_new_vote() {
	let proposer = acc(Sr25519Keyring::Alice);
	let curator = acc(Sr25519Keyring::Bob);
	let child_curator = acc(Sr25519Keyring::Charlie);
	let child_beneficiary = acc(Sr25519Keyring::Dave);
	let treasury = configs::TreasuryAccount::get();

	new_test_ext().execute_with(|| {
		Balances::set_balance(&proposer, 10_000 * UNIT);
		Balances::set_balance(&curator, 10_000 * UNIT);
		Balances::set_balance(&child_curator, 10_000 * UNIT);
		Balances::set_balance(&treasury, 1_000_000 * UNIT);

		let parent = active_parent_bounty(&proposer, &curator, 1_000 * UNIT, 100 * UNIT);

		let child_value = 100 * UNIT;
		let child_fee = 10 * UNIT;
		assert_ok!(ChildBounties::add_child_bounty(
			RuntimeOrigin::signed(curator.clone()),
			parent,
			child_value,
			b"sub task".to_vec(),
		));
		let child = 0;
		assert_ok!(ChildBounties::propose_curator(
			RuntimeOrigin::signed(curator),
			parent,
			child,
			src(&child_curator),
			child_fee,
		));
		assert_ok!(ChildBounties::accept_curator(
			RuntimeOrigin::signed(child_curator.clone()),
			parent,
			child,
		));
		assert_ok!(ChildBounties::award_child_bounty(
			RuntimeOrigin::signed(child_curator),
			parent,
			child,
			src(&child_beneficiary),
		));

		let delay = <Runtime as pallet_bounties::Config>::BountyDepositPayoutDelay::get();
		System::set_block_number(System::block_number() + delay + 1);
		assert_ok!(ChildBounties::claim_child_bounty(
			RuntimeOrigin::signed(proposer),
			parent,
			child,
		));

		assert_eq!(Balances::free_balance(&child_beneficiary), child_value - child_fee);
	});
}

#[test]
fn treasury_and_child_bounty_params_match_spec() {
	assert_eq!(<Runtime as pallet_treasury::Config>::Burn::get(), sp_runtime::Permill::zero());
	assert_eq!(<Runtime as pallet_child_bounties::Config>::ChildBountyValueMinimum::get(), UNIT);
	assert_eq!(<Runtime as pallet_child_bounties::Config>::MaxActiveChildBountyCount::get(), 100);
}
