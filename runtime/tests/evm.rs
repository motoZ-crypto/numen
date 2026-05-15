// Integration tests for the Frontier EVM stack wired into this runtime.
//
// These exercise the `AccountId32 ↔ H160` address mapping and confirm that a
// trivial Solidity-equivalent contract can be deployed through
// `pallet_evm::runner::stack::Runner` end-to-end.

use frame_support::traits::{tokens::fungible::Mutate, Get};
use pallet_evm::{AddressMapping, Runner};
use solochain_template_runtime::{configs, AccountId, Balances, Runtime, System, UNIT};
use sp_core::{H160, U256};
use sp_io::TestExternalities;
use sp_runtime::BuildStorage;

fn new_test_ext() -> TestExternalities {
	let t = frame_system::GenesisConfig::<Runtime>::default()
		.build_storage()
		.expect("GenesisConfig builds");
	let mut ext = TestExternalities::from(t);
	ext.execute_with(|| {
		System::set_block_number(1);
	});
	ext
}

#[test]
fn address_mapping_is_deterministic() {
	let h160 = H160::from_low_u64_be(0xdeadbeef);
	let a = <Runtime as pallet_evm::Config>::AddressMapping::into_account_id(h160);
	let b = <Runtime as pallet_evm::Config>::AddressMapping::into_account_id(h160);
	assert_eq!(a, b, "AddressMapping must be deterministic");

	// Different inputs → different accounts (collision-resistance smoke test).
	let other =
		<Runtime as pallet_evm::Config>::AddressMapping::into_account_id(H160::from_low_u64_be(1));
	assert_ne!(a, other);
}

#[test]
fn address_mapping_matches_blake2_evm_hash() {
	use sp_core::Hasher as _;
	use sp_runtime::traits::BlakeTwo256;

	let h160 = H160::repeat_byte(0xAB);
	let mapped = <Runtime as pallet_evm::Config>::AddressMapping::into_account_id(h160);

	// Frontier's `HashedAddressMapping<BlakeTwo256>` produces
	// `AccountId32(blake2_256("evm:" ++ h160))`.
	let mut payload = [0u8; 4 + 20];
	payload[0..4].copy_from_slice(b"evm:");
	payload[4..].copy_from_slice(h160.as_bytes());
	let expected_hash = BlakeTwo256::hash(&payload);
	let expected: AccountId = sp_runtime::AccountId32::from(<[u8; 32]>::from(expected_hash));

	assert_eq!(mapped, expected);
}

#[test]
fn deploy_minimal_contract_succeeds() {
	new_test_ext().execute_with(|| {
		let caller = H160::from_low_u64_be(0xCAFE);
		let caller_account =
			<Runtime as pallet_evm::Config>::AddressMapping::into_account_id(caller);

		// Endow the substrate account that backs the EVM caller with enough
		// balance to cover gas + value transfers.
		Balances::set_balance(&caller_account, 1_000_000_000 * UNIT);

		// Init bytecode that returns a 1-byte runtime: `STOP` (0x00).
		//   60 01           PUSH1 0x01      (size of runtime code)
		//   60 0c           PUSH1 0x0c      (offset in code = 12)
		//   60 00           PUSH1 0x00      (memory destination)
		//   39              CODECOPY
		//   60 01           PUSH1 0x01      (return size)
		//   60 00           PUSH1 0x00      (return offset)
		//   f3              RETURN
		//   00              STOP            (runtime code byte 12)
		let init_code: Vec<u8> =
			vec![0x60, 0x01, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, 0x01, 0x60, 0x00, 0xf3, 0x00];

		let max_fee_per_gas = U256::from(2_000_000_000u64); // 2 gwei, > base fee

		let result = <Runtime as pallet_evm::Config>::Runner::create(
			caller,
			init_code,
			U256::zero(),
			1_000_000,
			Some(max_fee_per_gas),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true,
			true,
			None,
			None,
			<Runtime as pallet_evm::Config>::config(),
		)
		.expect("EVM create must dispatch without runtime error");

		assert!(
			result.exit_reason.is_succeed(),
			"contract creation must succeed: {:?}",
			result.exit_reason
		);

		// Verify the EVM recorded the expected runtime code at the returned address.
		let code = pallet_evm::AccountCodes::<Runtime>::get(result.value);
		assert_eq!(code, vec![0x00], "deployed runtime code must be 0x00 (STOP)");
	});
}

#[test]
fn balances_erc20_precompile_balance_of_and_transfer_via_runner() {
	// End-to-end smoke test for the `balances-erc20` precompile wired at
	// 0x0802 in the runtime: drive a real `Runner::call` through Frontier's
	// stack and assert that
	//   1) `balanceOf(caller)` returns the funded native balance, and
	//   2) `transfer(other, value)` moves native funds across the
	//      `AddressMapping` mirror accounts.
	new_test_ext().execute_with(|| {
		let caller = H160::from_low_u64_be(0xC1A1);
		let other = H160::from_low_u64_be(0xB0B0);
		let caller_acc =
			<Runtime as pallet_evm::Config>::AddressMapping::into_account_id(caller);
		let other_acc =
			<Runtime as pallet_evm::Config>::AddressMapping::into_account_id(other);

		let initial: u128 = 7_500 * UNIT;
		Balances::set_balance(&caller_acc, initial);

		let precompile = H160::from_low_u64_be(0x0802);
		let max_fee_per_gas = U256::from(2_000_000_000u64); // 2 gwei

		// --- balanceOf(caller) ------------------------------------------------
		// Selector = keccak256("balanceOf(address)")[..4] = 0x70a08231.
		let mut data = vec![0x70, 0xa0, 0x82, 0x31];
		data.extend_from_slice(&[0u8; 12]);
		data.extend_from_slice(caller.as_bytes());

		let res = <Runtime as pallet_evm::Config>::Runner::call(
			caller,
			precompile,
			data,
			U256::zero(),
			1_000_000,
			Some(max_fee_per_gas),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true,
			true,
			None,
			None,
			None,
			<Runtime as pallet_evm::Config>::config(),
		)
		.expect("balanceOf call must dispatch without runtime error");
		assert!(
			res.exit_reason.is_succeed(),
			"balanceOf must succeed: {:?}",
			res.exit_reason
		);
		assert_eq!(res.value.len(), 32, "balanceOf returns one ABI word");
		let returned = U256::from_big_endian(&res.value);
		// EVM withholds `gas_limit * gas_price` from the caller's mirror
		// account before the precompile body executes, so `balanceOf`
		// observes the pre-refund balance — strictly less than initial,
		// but within one UNIT of it.
		let initial_u = U256::from(initial);
		assert!(returned < initial_u, "gas must have been withheld");
		assert!(initial_u - returned < U256::from(UNIT), "withheld gas under 1 UNIT");

		// --- transfer(other, 1_000 * UNIT) ------------------------------------
		// Selector = keccak256("transfer(address,uint256)")[..4] = 0xa9059cbb.
		let amount: u128 = 1_000 * UNIT;
		let mut data = vec![0xa9, 0x05, 0x9c, 0xbb];
		data.extend_from_slice(&[0u8; 12]);
		data.extend_from_slice(other.as_bytes());
		let amount_word = U256::from(amount).to_big_endian();
		data.extend_from_slice(&amount_word);

		let res = <Runtime as pallet_evm::Config>::Runner::call(
			caller,
			precompile,
			data,
			U256::zero(),
			1_000_000,
			Some(max_fee_per_gas),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true,
			true,
			None,
			None,
			None,
			<Runtime as pallet_evm::Config>::config(),
		)
		.expect("transfer call must dispatch without runtime error");
		assert!(
			res.exit_reason.is_succeed(),
			"transfer must succeed: {:?}",
			res.exit_reason
		);
		// Solidity returns `bool true` as a 32-byte word with low byte = 1.
		assert_eq!(U256::from_big_endian(&res.value), U256::one());

		// Recipient mirror account credited; sender mirror account decremented
		// (note: gas fee is also charged to caller_acc, so use `<=`).
		assert_eq!(Balances::free_balance(&other_acc), amount);
		assert!(Balances::free_balance(&caller_acc) <= initial - amount);
	});
}

#[test]
fn evm_chain_id_constant_matches_genesis_value() {
	// The runtime parameter feeds `pallet-evm-chain-id` via the genesis preset;
	// verify the source-of-truth constant is the value we configured.
	assert_eq!(configs::evm::ChainId::get(), 32_026u64);

	// And under externalities, `pallet-evm-chain-id` reads from storage —
	// without genesis init it returns the default (0).
	new_test_ext().execute_with(|| {
		let id = <Runtime as pallet_evm::Config>::ChainId::get();
		assert_eq!(id, 0, "default storage value when no genesis preset is applied");
	});
}

/// The six well-known Frontier dev EVM addresses (Alith, Baltathar, Charleth,
/// Dorothy, Ethan, Faith). Their publicly documented private keys ship with
/// every Frontier node template, so Hardhat / Foundry / MetaMask can use them
/// directly against any preset that pre-funds them.
const DEV_EVM_ADDRESSES: [&str; 6] = [
	"0xf24ff3a9cf04c71dbc94d0b566f7a27b94566cac", // Alith
	"0x3cd0a705a2dc65e5b1e1205896baa2be8a07c6e0", // Baltathar
	"0x798d4ba9baf0064ec19eb4f0a1a45785ae9d6dfc", // Charleth
	"0x773539d4ac0e786233d90a233654ccee26a613d9", // Dorothy
	"0xff64d3f6efe2317ee2807d223a0bdc4c0c49dfdb", // Ethan
	"0xc0f0f4ab324c46e55d02d0033343b4be8a55532d", // Faith
];

fn assert_preset_pre_funds_dev_evm_accounts(preset: &str) {
	use solochain_template_runtime::genesis_config_presets::get_preset;
	use sp_genesis_builder::PresetId;

	let raw = get_preset(&PresetId::from(preset))
		.unwrap_or_else(|| panic!("preset `{preset}` must exist"));
	let json: serde_json::Value =
		serde_json::from_slice(&raw).expect("preset emits valid JSON");

	let accounts = json
		.get("evm")
		.and_then(|v| v.get("accounts"))
		.and_then(|v| v.as_object())
		.unwrap_or_else(|| panic!("`evm.accounts` missing in `{preset}` preset"));

	assert_eq!(
		accounts.len(),
		DEV_EVM_ADDRESSES.len(),
		"`{preset}` preset must pre-fund exactly {} dev EVM accounts",
		DEV_EVM_ADDRESSES.len(),
	);
	for addr in DEV_EVM_ADDRESSES {
		let entry = accounts
			.get(addr)
			.unwrap_or_else(|| panic!("dev EVM account {addr} not pre-funded by `{preset}`"));
		let balance_str = entry
			.get("balance")
			.and_then(|v| v.as_str())
			.expect("balance serialized as hex string");
		let balance = sp_core::U256::from_str_radix(
			balance_str.trim_start_matches("0x"),
			16,
		)
		.expect("balance parses as U256");
		assert_eq!(
			balance,
			sp_core::U256::from(1_000_000u128 * UNIT),
			"unexpected starting balance for {addr} in `{preset}`",
		);
	}
}

#[test]
fn dev_preset_pre_funds_frontier_dev_evm_accounts() {
	assert_preset_pre_funds_dev_evm_accounts(sp_genesis_builder::DEV_RUNTIME_PRESET);
}

#[test]
fn local_and_integration_presets_pre_fund_frontier_dev_evm_accounts() {
	assert_preset_pre_funds_dev_evm_accounts(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET);
	assert_preset_pre_funds_dev_evm_accounts(
		solochain_template_runtime::genesis_config_presets::INTEGRATION_RUNTIME_PRESET,
	);
}

#[test]
fn balances_erc20_precompile_withdraw_moves_funds_back_to_substrate() {
	// `withdraw(bytes32 dest, uint256 amount)` on the balances-erc20
	// precompile is the supported way to move native UNIT from an EVM
	// caller back to a substrate `AccountId32` that has no `H160`
	// preimage. Verify that a single end-to-end EVM call:
	//   1) credits the destination substrate account by exactly `amount`,
	//   2) debits the caller's mirror account by exactly `amount` plus
	//      the gas burned by `pallet-evm` (so the total balance moved out
	//      of the caller equals `amount + (issuance_before - issuance_after)`).
	new_test_ext().execute_with(|| {
		let caller = H160::from_low_u64_be(0xC1A1);
		let caller_acc =
			<Runtime as pallet_evm::Config>::AddressMapping::into_account_id(caller);

		// A pure substrate destination — i.e. an `AccountId32` whose 32
		// bytes do NOT correspond to any `HashedAddressMapping(H160)`
		// preimage. Mirrors the production case of withdrawing to e.g.
		// Alice or sudo.
		let dest_bytes: [u8; 32] = [0x42; 32];
		let dest_acc: AccountId = sp_runtime::AccountId32::from(dest_bytes);

		let initial: u128 = 7_500 * UNIT;
		let amount: u128 = 1_000 * UNIT;
		Balances::set_balance(&caller_acc, initial);
		assert_eq!(Balances::free_balance(&dest_acc), 0);

		let issuance_before = Balances::total_issuance();

		let precompile = H160::from_low_u64_be(0x0802);
		let max_fee_per_gas = U256::from(2_000_000_000u64);

		// Selector = keccak256("withdraw(bytes32,uint256)")[..4] = 0x040cf020.
		let mut data = vec![0x04, 0x0c, 0xf0, 0x20];
		data.extend_from_slice(&dest_bytes);
		let amount_word = U256::from(amount).to_big_endian();
		data.extend_from_slice(&amount_word);

		let res = <Runtime as pallet_evm::Config>::Runner::call(
			caller,
			precompile,
			data,
			U256::zero(),
			1_000_000,
			Some(max_fee_per_gas),
			None,
			None,
			Vec::new(),
			Vec::new(),
			true,
			true,
			None,
			None,
			None,
			<Runtime as pallet_evm::Config>::config(),
		)
		.expect("withdraw call must dispatch without runtime error");
		assert!(
			res.exit_reason.is_succeed(),
			"withdraw must succeed: {:?}",
			res.exit_reason,
		);
		// Solidity returns `bool true`.
		assert_eq!(U256::from_big_endian(&res.value), U256::one());

		// Substrate destination credited by exactly `amount`.
		assert_eq!(
			Balances::free_balance(&dest_acc),
			amount,
			"destination substrate account must be credited by amount"
		);
		// Caller mirror debited by exactly `amount` plus whatever gas
		// `pallet-evm` burned during the call. Equivalently: the total
		// balance moved out of the caller equals `amount` (transferred
		// to `dest_acc`) plus the issuance delta (burned as fees).
		let caller_after = Balances::free_balance(&caller_acc);
		let issuance_after = Balances::total_issuance();
		let gas_burn = issuance_before - issuance_after;
		assert_eq!(
			initial - caller_after,
			amount + gas_burn,
			"caller mirror debit must equal amount + gas burn",
		);
	});
}

