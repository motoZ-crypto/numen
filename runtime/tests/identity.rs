//! Identity wiring. Registrar and force entry points answer only to the prime
//! key, deposits are priced per encoded byte, and a killed identity's deposit
//! lands in the treasury.

mod common;

use codec::Encode;
use common::new_test_ext;
use frame_support::{
	assert_noop, assert_ok,
	traits::tokens::fungible::{Inspect, Mutate},
};
use numen_runtime::{
	AccountId, Balance, Balances, Identity, Runtime, RuntimeOrigin, Treasury, UNIT,
};
use pallet_identity::legacy::IdentityInfo;
use sp_keyring::Sr25519Keyring;
use sp_runtime::{traits::StaticLookup, DispatchError};

const FUNDS: Balance = 10_000 * UNIT;

type IdInfo = <Runtime as pallet_identity::Config>::IdentityInformation;

fn install_prime() -> AccountId {
	let key = Sr25519Keyring::Ferdie.to_account_id();
	pallet_prime::Key::<Runtime>::put(&key);
	key
}

fn src(who: &AccountId) -> <<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source {
	<Runtime as frame_system::Config>::Lookup::unlookup(who.clone())
}

/// An identity carrying a display name so the encoded size exceeds the empty
/// baseline, exercising the per-byte deposit.
fn identity_info() -> IdInfo {
	IdentityInfo {
		additional: Default::default(),
		display: pallet_identity::Data::Raw(b"numen-dev".to_vec().try_into().unwrap()),
		legal: Default::default(),
		web: Default::default(),
		riot: Default::default(),
		email: Default::default(),
		pgp_fingerprint: None,
		image: Default::default(),
		twitter: Default::default(),
	}
}

fn expected_deposit(info: &IdInfo) -> Balance {
	let basic = <Runtime as pallet_identity::Config>::BasicDeposit::get();
	let byte = <Runtime as pallet_identity::Config>::ByteDeposit::get();
	basic + byte * info.encoded_size() as Balance
}

#[test]
fn add_registrar_accepts_prime_rejects_others() {
	new_test_ext().execute_with(|| {
		let key = install_prime();
		let stranger = Sr25519Keyring::Alice.to_account_id();

		assert_noop!(
			Identity::add_registrar(RuntimeOrigin::signed(stranger.clone()), src(&stranger)),
			DispatchError::BadOrigin,
		);
		assert_noop!(
			Identity::add_registrar(RuntimeOrigin::root(), src(&stranger)),
			DispatchError::BadOrigin,
		);
		assert!(pallet_identity::Registrars::<Runtime>::get().is_empty());

		assert_ok!(Identity::add_registrar(RuntimeOrigin::signed(key), src(&stranger)));
		assert_eq!(pallet_identity::Registrars::<Runtime>::get().len(), 1);
	});
}

#[test]
fn identity_deposit_prices_encoded_bytes() {
	new_test_ext().execute_with(|| {
		let who = Sr25519Keyring::Alice.to_account_id();
		Balances::set_balance(&who, FUNDS);
		let info = identity_info();

		assert_ok!(Identity::set_identity(
			RuntimeOrigin::signed(who.clone()),
			Box::new(info.clone()),
		));

		assert_eq!(Balances::reserved_balance(&who), expected_deposit(&info));
	});
}

#[test]
fn killed_identity_deposit_lands_in_treasury() {
	new_test_ext().execute_with(|| {
		let key = install_prime();
		let who = Sr25519Keyring::Alice.to_account_id();
		Balances::set_balance(&who, FUNDS);
		let info = identity_info();
		assert_ok!(Identity::set_identity(
			RuntimeOrigin::signed(who.clone()),
			Box::new(info.clone()),
		));
		let deposit = expected_deposit(&info);
		assert_eq!(Balances::reserved_balance(&who), deposit);

		let treasury = Treasury::account_id();
		let treasury_before = Balances::balance(&treasury);

		assert_ok!(Identity::kill_identity(RuntimeOrigin::signed(key), src(&who)));

		assert_eq!(Balances::reserved_balance(&who), 0);
		assert_eq!(Balances::balance(&treasury), treasury_before + deposit);
	});
}
