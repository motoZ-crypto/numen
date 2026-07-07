use crate::{
	AccountId, BalancesConfig, DifficultyConfig, EVMChainIdConfig, EVMConfig,
	RuntimeGenesisConfig, SessionConfig, SessionKeys, SudoConfig, ValidatorConfig, UNIT,
};
use alloc::{collections::BTreeMap, vec, vec::Vec};
use fp_evm::GenesisAccount;
use frame_support::build_struct_json_patch;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use serde_json::Value;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{H160, U256};
use sp_genesis_builder::{self, PresetId};
use sp_keyring::{Ed25519Keyring, Sr25519Keyring};

/// Per-account starting balance for the well-known Frontier dev EVM accounts
/// pre-funded by every preset (including `integration`). One million UNIT is
/// far more than any tooling test needs, while still leaving headroom against
/// the 1 billion UNIT total supply seeded into the substrate-side balances.
const DEV_EVM_ACCOUNT_BALANCE: u128 = 1_000_000 * UNIT;

/// Genesis treasury endowment. 600M NUMN seeded once into the on-chain treasury
/// pot, which has no ongoing income. Dev presets keep their own account funding
/// on top for testing.
const TREASURY_GENESIS_ENDOWMENT: u128 = 600_000_000 * UNIT;

/// Well-known Frontier dev ECDSA accounts pre-funded on the EVM side.
///
/// These six addresses (Alith, Baltathar, Charleth, Dorothy, Ethan, Faith)
/// have publicly documented private keys shipped with every Frontier-based
/// node template, so Hardhat / Foundry / MetaMask can drive the chain
/// out-of-the-box without first having to bridge funds from a substrate
/// account. They MUST NOT be used in any non-development network.
fn dev_evm_accounts() -> BTreeMap<H160, GenesisAccount> {
	let balance = U256::from(DEV_EVM_ACCOUNT_BALANCE);
	let make = |bytes: [u8; 20]| {
		(
			H160::from(bytes),
			GenesisAccount {
				nonce: U256::zero(),
				balance,
				storage: Default::default(),
				code: Default::default(),
			},
		)
	};
	[
		// Alith
		make([
			0xf2, 0x4f, 0xf3, 0xa9, 0xcf, 0x04, 0xc7, 0x1d, 0xbc, 0x94, 0xd0, 0xb5, 0x66, 0xf7,
			0xa2, 0x7b, 0x94, 0x56, 0x6c, 0xac,
		]),
		// Baltathar
		make([
			0x3c, 0xd0, 0xa7, 0x05, 0xa2, 0xdc, 0x65, 0xe5, 0xb1, 0xe1, 0x20, 0x58, 0x96, 0xba,
			0xa2, 0xbe, 0x8a, 0x07, 0xc6, 0xe0,
		]),
		// Charleth
		make([
			0x79, 0x8d, 0x4b, 0xa9, 0xba, 0xf0, 0x06, 0x4e, 0xc1, 0x9e, 0xb4, 0xf0, 0xa1, 0xa4,
			0x57, 0x85, 0xae, 0x9d, 0x6d, 0xfc,
		]),
		// Dorothy
		make([
			0x77, 0x35, 0x39, 0xd4, 0xac, 0x0e, 0x78, 0x62, 0x33, 0xd9, 0x0a, 0x23, 0x36, 0x54,
			0xcc, 0xee, 0x26, 0xa6, 0x13, 0xd9,
		]),
		// Ethan
		make([
			0xff, 0x64, 0xd3, 0xf6, 0xef, 0xe2, 0x31, 0x7e, 0xe2, 0x80, 0x7d, 0x22, 0x3a, 0x0b,
			0xdc, 0x4c, 0x0c, 0x49, 0xdf, 0xdb,
		]),
		// Faith
		make([
			0xc0, 0xf0, 0xf4, 0xab, 0x32, 0x4c, 0x46, 0xe5, 0x5d, 0x02, 0xd0, 0x03, 0x33, 0x43,
			0xb4, 0xbe, 0x8a, 0x55, 0x53, 0x2d,
		]),
	]
	.into_iter()
	.collect()
}

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
	let initial_difficulty = U256::from(1_000u64);
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
	let mut initial_balances: Vec<(AccountId, u128)> = endowed_accounts
		.iter()
		.cloned()
		.map(|k| (k, balance_per_account))
		.collect();
	initial_balances.push((crate::configs::TreasuryAccount::get(), TREASURY_GENESIS_ENDOWMENT));

	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: initial_balances,
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
		evm: EVMConfig {
			accounts: dev_evm_accounts(),
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
		vec![],
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
