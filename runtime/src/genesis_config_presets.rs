use crate::{
	AccountId, BalancesConfig, DifficultyConfig, EVMChainIdConfig, RuntimeGenesisConfig,
	SessionConfig, SessionKeys, SudoConfig, ValidatorConfig, UNIT,
};
use alloc::{vec, vec::Vec};
use frame_support::build_struct_json_patch;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use serde_json::Value;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::U256;
use sp_genesis_builder::{self, PresetId};
use sp_keyring::{Ed25519Keyring, Sr25519Keyring};

fn testnet_genesis(
	endowed_accounts: Vec<AccountId>,
	root: AccountId,
	initial_validators: Vec<(AccountId, GrandpaId, ImOnlineId)>,
) -> Value {
	testnet_genesis_with_extra_keys(endowed_accounts, root, initial_validators, Vec::new())
}

fn testnet_genesis_with_extra_keys(
	endowed_accounts: Vec<AccountId>,
	root: AccountId,
	initial_validators: Vec<(AccountId, GrandpaId, ImOnlineId)>,
	extra_session_keys: Vec<(AccountId, GrandpaId, ImOnlineId)>,
) -> Value {
	let total_supply: u128 = 1_000_000_000 * UNIT;
	let balance_per_account = total_supply / endowed_accounts.len() as u128;
	let initial_difficulty = U256::from(1_000_000u64);
	let validator_accounts: Vec<AccountId> =
		initial_validators.iter().map(|(a, _, _)| a.clone()).collect();
	let mut session_keys: Vec<(AccountId, AccountId, SessionKeys)> = initial_validators
		.into_iter()
		.map(|(account, grandpa, im_online)| {
			(account.clone(), account, SessionKeys { grandpa, im_online })
		})
		.collect();
	for (account, grandpa, im_online) in extra_session_keys.into_iter() {
		session_keys.push((account.clone(), account, SessionKeys { grandpa, im_online }));
	}
	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, balance_per_account))
				.collect::<Vec<_>>(),
		},
		sudo: SudoConfig { key: Some(root) },
		difficulty: DifficultyConfig {
			initial_difficulty,
			// anchor_target: U256::zero(),
			// anchor_timestamp: 0,
			// anchor_height: 0,
		},
		session: SessionConfig { keys: session_keys },
		validator: ValidatorConfig {
			initial_validators: validator_accounts,
			..Default::default()
		},
		evm_chain_id: EVMChainIdConfig {
			chain_id: 32026,
			..Default::default()
		},
	})
}

/// Derive an `ImOnlineId` from an Sr25519 dev keyring entry.
///
/// Heartbeat keys live under their own key type (`imon`) but the underlying
/// curve is sr25519; reusing the dev keyring keeps the dev/local presets
/// reproducible and matches the keys that `--alice`-style flags insert.
fn im_online_from_keyring(keyring: Sr25519Keyring) -> ImOnlineId {
	keyring.public().into()
}

pub fn development_config_genesis() -> Value {
	testnet_genesis(
		vec![
			Sr25519Keyring::Alice.to_account_id(),
			Sr25519Keyring::Bob.to_account_id(),
			Sr25519Keyring::Charlie.to_account_id(),
			Sr25519Keyring::AliceStash.to_account_id(),
			Sr25519Keyring::BobStash.to_account_id(),
		],
		Sr25519Keyring::Alice.to_account_id(),
		vec![
			(
				Sr25519Keyring::Alice.to_account_id(),
				Ed25519Keyring::Alice.public().into(),
				im_online_from_keyring(Sr25519Keyring::Alice),
			),
			(
				Sr25519Keyring::Bob.to_account_id(),
				Ed25519Keyring::Bob.public().into(),
				im_online_from_keyring(Sr25519Keyring::Bob),
			),
			(
				Sr25519Keyring::Charlie.to_account_id(),
				Ed25519Keyring::Charlie.public().into(),
				im_online_from_keyring(Sr25519Keyring::Charlie),
			),
		],
	)
}

pub fn local_config_genesis() -> Value {
	testnet_genesis(
		Sr25519Keyring::iter()
			.filter(|v| v != &Sr25519Keyring::One && v != &Sr25519Keyring::Two)
			.map(|v| v.to_account_id())
			.collect::<Vec<_>>(),
		Sr25519Keyring::Alice.to_account_id(),
		vec![
			(
				Sr25519Keyring::Alice.to_account_id(),
				Ed25519Keyring::Alice.public().into(),
				im_online_from_keyring(Sr25519Keyring::Alice),
			),
			(
				Sr25519Keyring::Bob.to_account_id(),
				Ed25519Keyring::Bob.public().into(),
				im_online_from_keyring(Sr25519Keyring::Bob),
			),
			(
				Sr25519Keyring::Charlie.to_account_id(),
				Ed25519Keyring::Charlie.public().into(),
				im_online_from_keyring(Sr25519Keyring::Charlie),
			),
		],
	)
}

pub fn integration_config_genesis() -> Value {
	testnet_genesis_with_extra_keys(
		vec![
			Sr25519Keyring::Alice.to_account_id(),
			Sr25519Keyring::Bob.to_account_id(),
			Sr25519Keyring::Charlie.to_account_id(),
			Sr25519Keyring::Dave.to_account_id(),
			Sr25519Keyring::Eve.to_account_id(),
			Sr25519Keyring::Ferdie.to_account_id(),
			Sr25519Keyring::AliceStash.to_account_id(),
			Sr25519Keyring::BobStash.to_account_id(),
		],
		Sr25519Keyring::Alice.to_account_id(),
		vec![
			(
				Sr25519Keyring::Alice.to_account_id(),
				Ed25519Keyring::Alice.public().into(),
				im_online_from_keyring(Sr25519Keyring::Alice),
			),
			(
				Sr25519Keyring::Bob.to_account_id(),
				Ed25519Keyring::Bob.public().into(),
				im_online_from_keyring(Sr25519Keyring::Bob),
			),
			(
				Sr25519Keyring::Charlie.to_account_id(),
				Ed25519Keyring::Charlie.public().into(),
				im_online_from_keyring(Sr25519Keyring::Charlie),
			),
		],
		// Pre-register session keys for Dave and Eve so they can be
		// promoted to active validators at runtime via `validator.lock()`
		// without first calling `session.set_keys()`.
		vec![
			(
				Sr25519Keyring::Dave.to_account_id(),
				Ed25519Keyring::Dave.public().into(),
				im_online_from_keyring(Sr25519Keyring::Dave),
			),
			(
				Sr25519Keyring::Eve.to_account_id(),
				Ed25519Keyring::Eve.public().into(),
				im_online_from_keyring(Sr25519Keyring::Eve),
			),
		],
	)
}

pub const INTEGRATION_RUNTIME_PRESET: &str = "integration";

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	let patch = match id.as_ref() {
		sp_genesis_builder::DEV_RUNTIME_PRESET => development_config_genesis(),
		sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => local_config_genesis(),
		INTEGRATION_RUNTIME_PRESET => integration_config_genesis(),
		_ => return None,
	};
	Some(
		serde_json::to_string(&patch)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
	vec![
		PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
		PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
		PresetId::from(INTEGRATION_RUNTIME_PRESET),
	]
}
