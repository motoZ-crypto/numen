use crate::{mock::*, Error, Event, Key};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::{BuildStorage, DispatchError};
use sp_version::RuntimeVersion;

fn upgrade_version(spec_version: u32) -> RuntimeVersion {
	RuntimeVersion {
		spec_name: "test".into(),
		spec_version,
		..Default::default()
	}
}

#[test]
fn genesis_key_lands_in_storage() {
	new_test_ext().execute_with(|| {
		assert_eq!(Key::<Test>::get(), Some(PRIME));
	});
}

#[test]
fn empty_key_rejects_everyone() {
	let t = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap();
	sp_io::TestExternalities::from(t).execute_with(|| {
		assert_noop!(
			Prime::set_key(RuntimeOrigin::signed(PRIME), OTHER),
			Error::<Test>::RequirePrime,
		);
	});
}

#[test]
fn upgrade_replaces_runtime_code() {
	new_test_ext_with_version(upgrade_version(2)).execute_with(|| {
		assert_ok!(Prime::upgrade(RuntimeOrigin::signed(PRIME), vec![1, 2, 3]));
		System::assert_has_event(frame_system::Event::CodeUpdated.into());
	});
}

#[test]
fn upgrade_keeps_system_version_checks() {
	new_test_ext_with_version(upgrade_version(1)).execute_with(|| {
		assert_noop!(
			Prime::upgrade(RuntimeOrigin::signed(PRIME), vec![1, 2, 3]),
			frame_system::Error::<Test>::SpecVersionNeedsToIncrease,
		);
	});
}

#[test]
fn upgrade_rejects_non_prime() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Prime::upgrade(RuntimeOrigin::signed(OTHER), vec![1, 2, 3]),
			Error::<Test>::RequirePrime,
		);
	});
}

#[test]
fn upgrade_rejects_unsigned_origins() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Prime::upgrade(RuntimeOrigin::root(), vec![1, 2, 3]),
			DispatchError::BadOrigin,
		);
		assert_noop!(
			Prime::upgrade(RuntimeOrigin::none(), vec![1, 2, 3]),
			DispatchError::BadOrigin,
		);
	});
}

#[test]
fn set_key_rotates_key() {
	new_test_ext().execute_with(|| {
		assert_ok!(Prime::set_key(RuntimeOrigin::signed(PRIME), OTHER));
		assert_eq!(Key::<Test>::get(), Some(OTHER));
		System::assert_last_event(Event::<Test>::KeyChanged { old: PRIME, new: OTHER }.into());

		assert_noop!(
			Prime::set_key(RuntimeOrigin::signed(PRIME), PRIME),
			Error::<Test>::RequirePrime,
		);
		assert_ok!(Prime::set_key(RuntimeOrigin::signed(OTHER), PRIME));
	});
}

#[test]
fn set_key_rejects_non_prime() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Prime::set_key(RuntimeOrigin::signed(OTHER), OTHER),
			Error::<Test>::RequirePrime,
		);
	});
}
