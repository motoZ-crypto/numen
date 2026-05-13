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
