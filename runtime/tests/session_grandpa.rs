//! Session ↔ GRANDPA wiring integration tests.
//!
//! Builds a minimal mock runtime composed of `frame-system`, `pallet-balances`,
//! `pallet-timestamp`, `pallet-session` and `pallet-grandpa` and verifies that
//! GRANDPA authority transitions happen at session boundaries (FR-SES-004).

#![cfg(test)]

use std::cell::RefCell;

use codec::Decode;
use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{ConstU128, ConstU32, ConstU64, OnFinalize, OnInitialize},
};
use pallet_session::{PeriodicSessions, SessionManager};
use sp_consensus_grandpa::{AuthorityId as GrandpaId, ConsensusLog, GRANDPA_ENGINE_ID};
use sp_keyring::Ed25519Keyring;
use sp_runtime::{impl_opaque_keys, traits::ConvertInto, BuildStorage, DigestItem};

type AccountId = u64;
type Balance = u128;
type BlockNumber = u64;
type Block = frame_system::mocking::MockBlock<Test>;

const SESSION_PERIOD: BlockNumber = 5;

impl_opaque_keys! {
	pub struct MockKeys {
		pub grandpa: Grandpa,
	}
}

construct_runtime!(
	pub enum Test {
		System: frame_system,
		Timestamp: pallet_timestamp,
		Balances: pallet_balances,
		Session: pallet_session,
		Grandpa: pallet_grandpa,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId;
	type Lookup = sp_runtime::traits::IdentityLookup<AccountId>;
	type AccountData = pallet_balances::AccountData<Balance>;
}

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<1>;
	type WeightInfo = ();
}

impl pallet_balances::Config for Test {
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = System;
	type WeightInfo = ();
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxFreezes = frame_support::traits::VariantCountOf<RuntimeFreezeReason>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type DoneSlashHandler = ();
}

parameter_types! {
	pub const SessionPeriodParam: BlockNumber = SESSION_PERIOD;
	pub const SessionOffsetParam: BlockNumber = 0;
}

thread_local! {
	/// Validator set returned by the mock `SessionManager` for the next session.
	static NEXT_VALIDATORS: RefCell<Option<Vec<AccountId>>> = RefCell::new(None);
	/// The most recently returned validator set. Used to mimic the real-world
	/// convention where `SessionManager::new_session` returns `None` when the
	/// validator set has not changed.
	static LAST_RETURNED: RefCell<Option<Vec<AccountId>>> = RefCell::new(None);
	/// Captured `(start_index, validators)` whenever `new_session` is called.
	static NEW_SESSION_LOG: RefCell<Vec<(u32, Vec<AccountId>)>> = RefCell::new(Vec::new());
}

pub struct TestSessionManager;

impl SessionManager<AccountId> for TestSessionManager {
	fn new_session(new_index: u32) -> Option<Vec<AccountId>> {
		let next = NEXT_VALIDATORS.with(|cell| cell.borrow().clone());
		let last = LAST_RETURNED.with(|cell| cell.borrow().clone());
		if next == last {
			// Set unchanged since last query; signal "no change" so that
			// pallet-session does not flag the session as changed.
			return None;
		}
		LAST_RETURNED.with(|cell| *cell.borrow_mut() = next.clone());
		if let Some(ref v) = next {
			NEW_SESSION_LOG.with(|cell| cell.borrow_mut().push((new_index, v.clone())));
		}
		next
	}
	fn end_session(_end_index: u32) {}
	fn start_session(_start_index: u32) {}
}

impl pallet_session::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = AccountId;
	type ValidatorIdOf = ConvertInto;
	type ShouldEndSession = PeriodicSessions<SessionPeriodParam, SessionOffsetParam>;
	type NextSessionRotation = PeriodicSessions<SessionPeriodParam, SessionOffsetParam>;
	type SessionManager = TestSessionManager;
	type SessionHandler = <MockKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
	type Keys = MockKeys;
	type DisablingStrategy = ();
	type WeightInfo = ();
	type Currency = Balances;
	type KeyDeposit = ConstU128<0>;
}

impl pallet_grandpa::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type MaxAuthorities = ConstU32<32>;
	type MaxNominators = ConstU32<0>;
	type MaxSetIdSessionEntries = ConstU64<32>;
	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn alice() -> AccountId {
	1
}
fn bob() -> AccountId {
	2
}
fn charlie() -> AccountId {
	3
}
fn dave() -> AccountId {
	4
}

fn keys_of(account: AccountId) -> MockKeys {
	let grandpa: GrandpaId = match account {
		1 => Ed25519Keyring::Alice.public().into(),
		2 => Ed25519Keyring::Bob.public().into(),
		3 => Ed25519Keyring::Charlie.public().into(),
		4 => Ed25519Keyring::Dave.public().into(),
		other => panic!("unexpected account {other}"),
	};
	MockKeys { grandpa }
}

fn grandpa_id(account: AccountId) -> GrandpaId {
	keys_of(account).grandpa
}

fn set_next_validators(set: Vec<AccountId>) {
	NEXT_VALIDATORS.with(|cell| *cell.borrow_mut() = Some(set));
}

fn clear_session_log() {
	NEW_SESSION_LOG.with(|cell| cell.borrow_mut().clear());
}

fn reset_session_manager_state() {
	NEXT_VALIDATORS.with(|cell| *cell.borrow_mut() = None);
	LAST_RETURNED.with(|cell| *cell.borrow_mut() = None);
	NEW_SESSION_LOG.with(|cell| cell.borrow_mut().clear());
}

fn new_test_ext(initial: Vec<AccountId>) -> sp_io::TestExternalities {
	reset_session_manager_state();
	set_next_validators(initial.clone());
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	// Register session keys for the full keyring so tests can transition to
	// any subset without having to call `set_keys` first.
	let all_accounts: Vec<AccountId> = vec![alice(), bob(), charlie(), dave()];
	pallet_balances::GenesisConfig::<Test> {
		balances: all_accounts.iter().map(|a| (*a, 1_000)).collect(),
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	pallet_session::GenesisConfig::<Test> {
		keys: initial.iter().map(|a| (*a, *a, keys_of(*a))).collect(),
		non_authority_keys: all_accounts
			.iter()
			.filter(|a| !initial.contains(a))
			.map(|a| (*a, *a, keys_of(*a)))
			.collect(),
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
	});
	ext
}

fn run_to_block(n: BlockNumber) {
	while System::block_number() < n {
		Session::on_finalize(System::block_number());
		Grandpa::on_finalize(System::block_number());
		System::on_finalize(System::block_number());
		System::reset_events();
		let next = System::block_number() + 1;
		System::set_block_number(next);
		System::on_initialize(next);
		Session::on_initialize(next);
		Grandpa::on_initialize(next);
	}
}

/// Finalize the current block so that pallet hooks that emit digests during
/// `on_finalize` (such as `pallet-grandpa`'s `ScheduledChange` log) take effect.
fn finalize_current_block() {
	let now = System::block_number();
	Session::on_finalize(now);
	Grandpa::on_finalize(now);
}

/// Returns the GRANDPA `ScheduledChange` log (if any) emitted in the current block.
fn grandpa_scheduled_change_log() -> Option<Vec<(GrandpaId, u64)>> {
	System::digest().logs.into_iter().find_map(|item| {
		if let DigestItem::Consensus(id, payload) = item {
			if id == GRANDPA_ENGINE_ID {
				if let Ok(ConsensusLog::ScheduledChange(change)) =
					ConsensusLog::<BlockNumber>::decode(&mut &payload[..])
				{
					return Some(change.next_authorities.into_iter().collect());
				}
			}
		}
		None
	})
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn genesis_authorities_match_initial_validators() {
	new_test_ext(vec![alice(), bob(), charlie()]).execute_with(|| {
		let auths: Vec<GrandpaId> =
			Grandpa::grandpa_authorities().into_iter().map(|(id, _w)| id).collect();
		assert_eq!(auths, vec![grandpa_id(alice()), grandpa_id(bob()), grandpa_id(charlie())]);
		assert_eq!(Grandpa::current_set_id(), 0);
		assert_eq!(Session::current_index(), 0);
	});
}

#[test]
fn session_rotates_every_period_blocks() {
	new_test_ext(vec![alice(), bob(), charlie()]).execute_with(|| {
		assert_eq!(Session::current_index(), 0);
		run_to_block(SESSION_PERIOD);
		// Exactly at the period boundary the new session has begun.
		assert_eq!(Session::current_index(), 1);
		run_to_block(SESSION_PERIOD * 2);
		assert_eq!(Session::current_index(), 2);
		run_to_block(SESSION_PERIOD * 3);
		assert_eq!(Session::current_index(), 3);
	});
}

#[test]
fn no_change_session_does_not_emit_scheduled_change() {
	new_test_ext(vec![alice(), bob(), charlie()]).execute_with(|| {
		// Validator set unchanged across the boundary.
		set_next_validators(vec![alice(), bob(), charlie()]);
		clear_session_log();
		run_to_block(SESSION_PERIOD * 2);
		finalize_current_block();
		assert!(
			grandpa_scheduled_change_log().is_none(),
			"unchanged validator set must not emit ScheduledChange"
		);
		assert_eq!(Grandpa::current_set_id(), 0);
	});
}

#[test]
fn validator_set_change_takes_effect_at_boundary() {
	new_test_ext(vec![alice(), bob(), charlie()]).execute_with(|| {
		// Mid-session change registration must not affect the current authority set.
		set_next_validators(vec![alice(), bob(), dave()]);
		run_to_block(SESSION_PERIOD - 1);
		let auths: Vec<GrandpaId> =
			Grandpa::grandpa_authorities().into_iter().map(|(id, _w)| id).collect();
		assert_eq!(
			auths,
			vec![grandpa_id(alice()), grandpa_id(bob()), grandpa_id(charlie())],
			"authority set must not change before session boundary"
		);
	});
}

#[test]
fn transition_abc_to_abd_smooth() {
	new_test_ext(vec![alice(), bob(), charlie()]).execute_with(|| {
		assert_eq!(Grandpa::current_set_id(), 0);

		set_next_validators(vec![alice(), bob(), dave()]);
		run_to_block(SESSION_PERIOD);
		finalize_current_block();

		// First boundary: ABD becomes queued (was ABC). pallet-session reports
		// `changed = false` for the active set, so no `ScheduledChange` yet.
		assert!(grandpa_scheduled_change_log().is_none());
		assert_eq!(Grandpa::current_set_id(), 0);

		run_to_block(SESSION_PERIOD * 2);
		finalize_current_block();

		// Second boundary: queued ABD becomes active, pallet-session reports
		// `changed = true`, pallet-grandpa schedules the authority change with
		// delay 0; both the `ScheduledChange` digest and the new authorities
		// are emitted in this block's `on_finalize`.
		let scheduled = grandpa_scheduled_change_log()
			.expect("changed validator set must emit a ScheduledChange digest");
		let scheduled_ids: Vec<GrandpaId> = scheduled.into_iter().map(|(id, _)| id).collect();
		assert_eq!(
			scheduled_ids,
			vec![grandpa_id(alice()), grandpa_id(bob()), grandpa_id(dave())]
		);
		assert_eq!(Grandpa::current_set_id(), 1, "set_id must increment on authority change");

		let auths: Vec<GrandpaId> =
			Grandpa::grandpa_authorities().into_iter().map(|(id, _w)| id).collect();
		assert_eq!(auths, vec![grandpa_id(alice()), grandpa_id(bob()), grandpa_id(dave())]);
	});
}

#[test]
fn empty_validator_set_handled_without_panic() {
	new_test_ext(vec![alice(), bob(), charlie()]).execute_with(|| {
		set_next_validators(vec![]);
		run_to_block(SESSION_PERIOD * 2);
		finalize_current_block();
		// Session rotation with empty next set must not panic. GRANDPA should
		// not advance to an empty authority set (it would brick finality), so
		// pallet-grandpa keeps the previous authorities and skips the change.
		// We only assert no panic and no state corruption here.
		let _ = Grandpa::grandpa_authorities();
		assert_eq!(Session::current_index(), 2);
	});
}
