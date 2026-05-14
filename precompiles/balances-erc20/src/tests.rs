//! Unit tests for the balances-erc20 precompile.
//!
//! Uses `precompile_utils::testing::PrecompilesTester` (`prepare_test`
//! chain API) to drive the precompile, mirroring the layout used by
//! Moonbeam's tests.

use frame_support::traits::Get;
use libsecp256k1::{Message, PublicKey, SecretKey};
use precompile_utils::{prelude::*, solidity, testing::*};
use sp_core::{H160, H256, U256};

use crate::{
	compute_domain_separator, compute_eip712_digest, compute_permit_struct_hash, mock::*,
	Allowances, Erc20Metadata, Nonces, SELECTOR_LOG_APPROVAL, SELECTOR_LOG_DEPOSIT,
	SELECTOR_LOG_TRANSFER, SELECTOR_LOG_WITHDRAWAL,
};

const ALICE: H160 = H160(hex_literal::hex!("1000000000000000000000000000000000000001"));
const BOB: H160 = H160(hex_literal::hex!("1000000000000000000000000000000000000002"));
const CHARLIE: H160 = H160(hex_literal::hex!("1000000000000000000000000000000000000003"));

// --- read methods ---------------------------------------------------------

#[test]
fn get_metadata_name() {
	ExtBuilder::default().build().execute_with(|| {
		precompiles()
			.prepare_test(ALICE, PRECOMPILE_ADDRESS, PCall::name {})
			.execute_returns(UnboundedBytes::from(NativeErc20Metadata::NAME.as_bytes()));
	});
}

#[test]
fn get_metadata_symbol() {
	ExtBuilder::default().build().execute_with(|| {
		precompiles()
			.prepare_test(ALICE, PRECOMPILE_ADDRESS, PCall::symbol {})
			.execute_returns(UnboundedBytes::from(NativeErc20Metadata::SYMBOL.as_bytes()));
	});
}

#[test]
fn get_metadata_decimals() {
	ExtBuilder::default().build().execute_with(|| {
		precompiles()
			.prepare_test(ALICE, PRECOMPILE_ADDRESS, PCall::decimals {})
			.execute_returns(NativeErc20Metadata::DECIMALS);
	});
}

#[test]
fn get_total_supply_single_account() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000), (mirror(BOB), 500)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(ALICE, PRECOMPILE_ADDRESS, PCall::total_supply {})
				.execute_returns(U256::from(1_500u64));
		});
}

#[test]
fn get_balances_known_user() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 7_777)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(
					BOB,
					PRECOMPILE_ADDRESS,
					PCall::balance_of { owner: Address(ALICE) },
				)
				.execute_returns(U256::from(7_777u64));
		});
}

// --- transfer ------------------------------------------------------------

#[test]
fn transfer_moves_funds_and_emits_log() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::transfer { to: Address(BOB), value: 400u64.into() },
				)
				.expect_log(log3(
					PRECOMPILE_ADDRESS,
					SELECTOR_LOG_TRANSFER,
					ALICE,
					BOB,
					solidity::encode_event_data(U256::from(400u64)),
				))
				.execute_returns(true);

			assert_eq!(Balances::free_balance(mirror(ALICE)), 600);
			assert_eq!(Balances::free_balance(mirror(BOB)), 400);
		});
}

#[test]
fn transfer_with_zero_amount_succeeds_without_balance_change() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 100)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::transfer { to: Address(BOB), value: U256::zero() },
				)
				.execute_returns(true);
			assert_eq!(Balances::free_balance(mirror(ALICE)), 100);
			assert_eq!(Balances::free_balance(mirror(BOB)), 0);
		});
}

#[test]
fn transfer_reverts_on_insufficient_balance() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 10)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::transfer { to: Address(BOB), value: 1_000u64.into() },
				)
				.execute_reverts(|out| out == b"ERC20: transfer failed");
			assert_eq!(Balances::free_balance(mirror(ALICE)), 10);
		});
}

#[test]
fn transfer_reverts_on_overflowing_amount() {
	ExtBuilder::default().build().execute_with(|| {
		// `Balance` is `u128`; values exceeding `u128::MAX` cannot convert.
		let amount = U256::from(u128::MAX) + U256::one();
		precompiles()
			.prepare_test(
				ALICE,
				PRECOMPILE_ADDRESS,
				PCall::transfer { to: Address(BOB), value: amount },
			)
			.execute_reverts(|out| out == b"ERC20: amount overflow");
	});
}

// --- approve / allowance / transferFrom ----------------------------------

#[test]
fn approve_sets_allowance_and_emits_log() {
	ExtBuilder::default().build().execute_with(|| {
		precompiles()
			.prepare_test(
				ALICE,
				PRECOMPILE_ADDRESS,
				PCall::approve { spender: Address(BOB), value: 123u64.into() },
			)
			.expect_log(log3(
				PRECOMPILE_ADDRESS,
				SELECTOR_LOG_APPROVAL,
				ALICE,
				BOB,
				solidity::encode_event_data(U256::from(123u64)),
			))
			.execute_returns(true);
		assert_eq!(Allowances::get(ALICE, BOB), U256::from(123u64));
	});
}

#[test]
fn allowance_query_returns_stored_value() {
	ExtBuilder::default().build().execute_with(|| {
		Allowances::insert(ALICE, BOB, U256::from(99u64));
		precompiles()
			.prepare_test(
				CHARLIE,
				PRECOMPILE_ADDRESS,
				PCall::allowance { owner: Address(ALICE), spender: Address(BOB) },
			)
			.execute_returns(U256::from(99u64));
	});
}

#[test]
fn transfer_from_consumes_allowance() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000)])
		.build()
		.execute_with(|| {
			Allowances::insert(ALICE, BOB, U256::from(500u64));
			precompiles()
				.prepare_test(
					BOB,
					PRECOMPILE_ADDRESS,
					PCall::transfer_from {
						from: Address(ALICE),
						to: Address(CHARLIE),
						value: 400u64.into(),
					},
				)
				.execute_returns(true);
			assert_eq!(Allowances::get(ALICE, BOB), U256::from(100u64));
			assert_eq!(Balances::free_balance(mirror(ALICE)), 600);
			assert_eq!(Balances::free_balance(mirror(CHARLIE)), 400);
		});
}

#[test]
fn transfer_from_reverts_on_insufficient_allowance() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000)])
		.build()
		.execute_with(|| {
			Allowances::insert(ALICE, BOB, U256::from(100u64));
			precompiles()
				.prepare_test(
					BOB,
					PRECOMPILE_ADDRESS,
					PCall::transfer_from {
						from: Address(ALICE),
						to: Address(CHARLIE),
						value: 200u64.into(),
					},
				)
				.execute_reverts(|out| out == b"ERC20: insufficient allowance");
			assert_eq!(Allowances::get(ALICE, BOB), U256::from(100u64));
			assert_eq!(Balances::free_balance(mirror(ALICE)), 1_000);
		});
}

#[test]
fn transfer_from_with_self_skips_allowance() {
	// When `from == spender`, the precompile does not consult the
	// allowance map (matches OpenZeppelin's reference behaviour).
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::transfer_from {
						from: Address(ALICE),
						to: Address(BOB),
						value: 700u64.into(),
					},
				)
				.execute_returns(true);
			assert_eq!(Allowances::get(ALICE, ALICE), U256::zero());
			assert_eq!(Balances::free_balance(mirror(BOB)), 700);
		});
}

// --- withdraw bridge ------------------------------------------------------

#[test]
fn withdraw_moves_funds_to_substrate_account_and_emits_log() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000)])
		.build()
		.execute_with(|| {
			let dest_acc = aid(7);
			let dest_bytes: [u8; 32] = dest_acc.clone().into();
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::withdraw { dest: H256(dest_bytes), value: 300u64.into() },
				)
				.expect_log(log3(
					PRECOMPILE_ADDRESS,
					SELECTOR_LOG_WITHDRAWAL,
					ALICE,
					H256(dest_bytes),
					solidity::encode_event_data(U256::from(300u64)),
				))
				.execute_returns(true);
			assert_eq!(Balances::free_balance(mirror(ALICE)), 700);
			assert_eq!(Balances::free_balance(&dest_acc), 300);
		});
}

#[test]
fn withdraw_with_zero_amount_does_not_move_funds() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 100)])
		.build()
		.execute_with(|| {
			let dest_acc = aid(8);
			let dest_bytes: [u8; 32] = dest_acc.clone().into();
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::withdraw { dest: H256(dest_bytes), value: U256::zero() },
				)
				.execute_returns(true);
			assert_eq!(Balances::free_balance(mirror(ALICE)), 100);
			assert_eq!(Balances::free_balance(&dest_acc), 0);
		});
}

#[test]
fn withdraw_reverts_on_insufficient_balance() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 10)])
		.build()
		.execute_with(|| {
			let dest_bytes: [u8; 32] = aid(9).into();
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::withdraw { dest: H256(dest_bytes), value: 100u64.into() },
				)
				.execute_reverts(|out| out == b"ERC20: transfer failed");
		});
}

// --- selector dispatch / fallback ----------------------------------------

#[test]
fn empty_input_reverts() {
	// Empty input is routed to the fallback (deposit); without value it reverts.
	ExtBuilder::default().build().execute_with(|| {
		precompiles()
			.prepare_test(ALICE, PRECOMPILE_ADDRESS, alloc::vec::Vec::<u8>::new())
			.execute_reverts(|out| out == b"ERC20: cannot deposit zero");
	});
}

#[test]
fn unknown_selector_reverts() {
	// Unknown selectors fall through to deposit (#[precompile::fallback]);
	// without value they revert with the deposit's reason.
	ExtBuilder::default().build().execute_with(|| {
		precompiles()
			.prepare_test(ALICE, PRECOMPILE_ADDRESS, vec![0xde, 0xad, 0xbe, 0xef])
			.execute_reverts(|out| out == b"ERC20: cannot deposit zero");
	});
}

#[test]
fn balance_of_with_short_args_reverts() {
	ExtBuilder::default().build().execute_with(|| {
		// Selector present but no 32-byte address argument.
		let mut input = compute_selector("balanceOf(address)").to_be_bytes().to_vec();
		input.extend_from_slice(&[0u8; 16]);
		precompiles()
			.prepare_test(ALICE, PRECOMPILE_ADDRESS, input)
			.execute_reverts(|_| true);
	});
}

#[test]
fn balance_of_with_dirty_padding_returns_truncated() {
	// The Solidity ABI silently truncates the high 12 bytes of an address
	// word; `Address` codec follows that convention.
	ExtBuilder::default().build().execute_with(|| {
		let mut bad = [0u8; 32];
		bad[0] = 1;
		bad[12..].copy_from_slice(BOB.as_bytes());
		let mut input = compute_selector("balanceOf(address)").to_be_bytes().to_vec();
		input.extend_from_slice(&bad);
		precompiles()
			.prepare_test(ALICE, PRECOMPILE_ADDRESS, input)
			.execute_returns(U256::zero());
	});
}

#[test]
fn unknown_selector_without_value_still_reverts() {
	ExtBuilder::default().build().execute_with(|| {
		precompiles()
			.prepare_test(ALICE, PRECOMPILE_ADDRESS, vec![0xde, 0xad, 0xbe, 0xef])
			.execute_reverts(|_| true);
	});
}

// --- Moonbeam-style scenarios --------------------------------------------

#[test]
fn selectors_match_erc20_spec() {
	// The macro-generated dispatcher uses these canonical selectors;
	// pin the values so a refactor cannot drift.
	assert_eq!(compute_selector("balanceOf(address)"), 0x70a08231);
	assert_eq!(compute_selector("totalSupply()"), 0x18160ddd);
	assert_eq!(compute_selector("approve(address,uint256)"), 0x095ea7b3);
	assert_eq!(compute_selector("allowance(address,address)"), 0xdd62ed3e);
	assert_eq!(compute_selector("transfer(address,uint256)"), 0xa9059cbb);
	assert_eq!(
		compute_selector("transferFrom(address,address,uint256)"),
		0x23b872dd
	);
	assert_eq!(compute_selector("name()"), 0x06fdde03);
	assert_eq!(compute_selector("symbol()"), 0x95d89b41);
	assert_eq!(compute_selector("decimals()"), 0x313ce567);
	// Withdraw uses a non-standard signature (bytes32 destination), so its
	// selector deliberately differs from the WETH-style `withdraw(uint256)`
	// (`0xf3fef3a3`).
	assert_eq!(compute_selector("withdraw(bytes32,uint256)"), 0x040cf020);

	// Pin the canonical event topic hashes used by indexers.
	assert_eq!(
		SELECTOR_LOG_TRANSFER,
		hex_literal::hex!("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef")
	);
	assert_eq!(
		SELECTOR_LOG_APPROVAL,
		hex_literal::hex!("8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925")
	);
	assert_eq!(
		SELECTOR_LOG_WITHDRAWAL,
		hex_literal::hex!("4206db6775563d1043abfcf27cd0ecd19fcc464be574a1487fc95b24957a671a")
	);
}

#[test]
fn balance_of_unknown_user_returns_zero() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::balance_of { owner: Address(BOB) },
				)
				.execute_returns(U256::zero());
		});
}

#[test]
fn total_supply_with_two_accounts() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000), (mirror(BOB), 2_500)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(ALICE, PRECOMPILE_ADDRESS, PCall::total_supply {})
				.execute_returns(U256::from(3_500u64));
		});
}

#[test]
fn approve_saturating_max_uint256() {
	// `approve(spender, MAX)` round-trips since both on-chain and
	// Solidity sides use U256.
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::approve { spender: Address(BOB), value: U256::MAX },
				)
				.execute_returns(true);
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::allowance { owner: Address(ALICE), spender: Address(BOB) },
				)
				.execute_returns(U256::MAX);
		});
}

#[test]
fn transfer_updates_both_balances() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::transfer { to: Address(BOB), value: 400u64.into() },
				)
				.execute_returns(true);
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::balance_of { owner: Address(ALICE) },
				)
				.execute_returns(U256::from(600u64));
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::balance_of { owner: Address(BOB) },
				)
				.execute_returns(U256::from(400u64));
		});
}

#[test]
fn transfer_from_updates_both_balances_and_allowance_remainder() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::approve { spender: Address(BOB), value: 500u64.into() },
				)
				.execute_returns(true);
			precompiles()
				.prepare_test(
					BOB,
					PRECOMPILE_ADDRESS,
					PCall::transfer_from {
						from: Address(ALICE),
						to: Address(BOB),
						value: 400u64.into(),
					},
				)
				.execute_returns(true);
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::balance_of { owner: Address(ALICE) },
				)
				.execute_returns(U256::from(600u64));
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::balance_of { owner: Address(BOB) },
				)
				.execute_returns(U256::from(400u64));
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::allowance { owner: Address(ALICE), spender: Address(BOB) },
				)
				.execute_returns(U256::from(100u64));
		});
}

#[test]
fn withdraw_does_not_change_caller_native_balance_when_amount_zero() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::withdraw { dest: H256::zero(), value: U256::zero() },
				)
				.execute_returns(true);
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::balance_of { owner: Address(ALICE) },
				)
				.execute_returns(U256::from(1_000u64));
		});
}

// --- deposit (WETH-style) tests --------------------------------------------

#[test]
fn deposit_with_value_refunds_caller_and_emits_log() {
	// Simulate the EVM's pre-transfer by funding both Alice and the
	// precompile so the post-state matches a real EVM call.
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 500), (mirror(PRECOMPILE_ADDRESS), 500)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(ALICE, PRECOMPILE_ADDRESS, PCall::deposit {})
				.with_value(U256::from(500u64))
				.expect_log(log2(
					PRECOMPILE_ADDRESS,
					SELECTOR_LOG_DEPOSIT,
					ALICE,
					solidity::encode_event_data(U256::from(500u64)),
				))
				.execute_returns(());
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::balance_of { owner: Address(ALICE) },
				)
				.execute_returns(U256::from(1_000u64));
		});
}

#[test]
fn deposit_via_empty_calldata_receive_refunds_caller() {
	// Empty calldata + value triggers receive() which routes to deposit
	// via the macro's fallback handler.
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 750), (mirror(PRECOMPILE_ADDRESS), 250)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(ALICE, PRECOMPILE_ADDRESS, alloc::vec::Vec::<u8>::new())
				.with_value(U256::from(250u64))
				.expect_log(log2(
					PRECOMPILE_ADDRESS,
					SELECTOR_LOG_DEPOSIT,
					ALICE,
					solidity::encode_event_data(U256::from(250u64)),
				))
				.execute_returns(());
		});
}

#[test]
fn deposit_via_unknown_selector_with_value_falls_back_to_deposit() {
	// Unknown selector + value triggers fallback() which routes to deposit.
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 900), (mirror(PRECOMPILE_ADDRESS), 100)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(ALICE, PRECOMPILE_ADDRESS, vec![0x01, 0x23, 0x45, 0x67])
				.with_value(U256::from(100u64))
				.expect_log(log2(
					PRECOMPILE_ADDRESS,
					SELECTOR_LOG_DEPOSIT,
					ALICE,
					solidity::encode_event_data(U256::from(100u64)),
				))
				.execute_returns(());
		});
}

#[test]
fn deposit_with_zero_value_reverts() {
	ExtBuilder::default()
		.with_balances(vec![(mirror(ALICE), 1_000)])
		.build()
		.execute_with(|| {
			precompiles()
				.prepare_test(ALICE, PRECOMPILE_ADDRESS, PCall::deposit {})
				.execute_reverts(|out| out == b"ERC20: cannot deposit zero");
			precompiles()
				.prepare_test(
					ALICE,
					PRECOMPILE_ADDRESS,
					PCall::balance_of { owner: Address(ALICE) },
				)
				.execute_returns(U256::from(1_000u64));
		});
}

// --- EIP-2612 permit ------------------------------------------------------

/// Deterministic test secret. Yields a fixed address used by the permit tests.
fn permit_test_secret() -> SecretKey {
	SecretKey::parse(&[0xCDu8; 32]).expect("valid secret")
}

/// Address corresponding to `permit_test_secret()`.
fn permit_test_address(sk: &SecretKey) -> H160 {
	let pk = PublicKey::from_secret_key(sk);
	// 65-byte uncompressed (0x04 || X || Y); strip the leading byte.
	let serialized = pk.serialize();
	let hash = sp_io::hashing::keccak_256(&serialized[1..]);
	H160::from_slice(&hash[12..])
}

fn sign_permit(
	sk: &SecretKey,
	owner: H160,
	spender: H160,
	value: U256,
	nonce: U256,
	deadline: U256,
) -> (u8, [u8; 32], [u8; 32]) {
	let chain_id = <Runtime as pallet_evm::Config>::ChainId::get();
	let ds = compute_domain_separator(PRECOMPILE_ADDRESS, chain_id, NativeErc20Metadata::NAME.as_bytes());
	let sh = compute_permit_struct_hash(owner, spender, value, nonce, deadline);
	let digest = compute_eip712_digest(&ds, &sh);
	let msg = Message::parse(&digest);
	let (sig, rec_id) = libsecp256k1::sign(&msg, sk);
	let bytes = sig.serialize();
	let mut r = [0u8; 32];
	let mut s = [0u8; 32];
	r.copy_from_slice(&bytes[..32]);
	s.copy_from_slice(&bytes[32..]);
	(rec_id.serialize() + 27, r, s)
}

#[test]
fn nonces_returns_zero_for_unknown_owner() {
	ExtBuilder::default().build().execute_with(|| {
		precompiles()
			.prepare_test(
				ALICE,
				PRECOMPILE_ADDRESS,
				PCall::eip2612_nonces { owner: Address(ALICE) },
			)
			.execute_returns(U256::zero());
	});
}

#[test]
fn domain_separator_matches_expected() {
	ExtBuilder::default().build().execute_with(|| {
		let chain_id = <Runtime as pallet_evm::Config>::ChainId::get();
		let expected = H256(compute_domain_separator(PRECOMPILE_ADDRESS, chain_id, NativeErc20Metadata::NAME.as_bytes()));
		precompiles()
			.prepare_test(ALICE, PRECOMPILE_ADDRESS, PCall::eip2612_domain_separator {})
			.execute_returns(expected);
	});
}

#[test]
fn permit_valid_sets_allowance_and_bumps_nonce() {
	ExtBuilder::default().build().execute_with(|| {
		Timestamp::set_timestamp(1_000); // 1 second
		let sk = permit_test_secret();
		let owner = permit_test_address(&sk);
		let spender = BOB;
		let value = U256::from(500u64);
		let deadline = U256::from(10_000u64);
		let (v, r, s) = sign_permit(&sk, owner, spender, value, U256::zero(), deadline);
		// Anyone (CHARLIE) submits the permit on the owner's behalf.
		precompiles()
			.prepare_test(
				CHARLIE,
				PRECOMPILE_ADDRESS,
				PCall::eip2612_permit {
					owner: Address(owner),
					spender: Address(spender),
					value,
					deadline,
					v,
					r: H256(r),
					s: H256(s),
				},
			)
			.expect_log(log3(
				PRECOMPILE_ADDRESS,
				SELECTOR_LOG_APPROVAL,
				owner,
				spender,
				solidity::encode_event_data(value),
			))
			.execute_returns(());
		assert_eq!(Allowances::get(owner, spender), value);
		assert_eq!(Nonces::get(owner), U256::one());
	});
}

#[test]
fn permit_invalid_nonce_reverts() {
	ExtBuilder::default().build().execute_with(|| {
		Timestamp::set_timestamp(1_000);
		let sk = permit_test_secret();
		let owner = permit_test_address(&sk);
		// Sign with nonce=1 but on-chain nonce is 0.
		let (v, r, s) = sign_permit(&sk, owner, BOB, 500u64.into(), U256::one(), 10_000u64.into());
		precompiles()
			.prepare_test(
				CHARLIE,
				PRECOMPILE_ADDRESS,
				PCall::eip2612_permit {
					owner: Address(owner),
					spender: Address(BOB),
					value: 500u64.into(),
					deadline: 10_000u64.into(),
					v,
					r: H256(r),
					s: H256(s),
				},
			)
			.execute_reverts(|out| out == b"Invalid permit");
		assert_eq!(Nonces::get(owner), U256::zero());
	});
}

#[test]
fn permit_invalid_signature_reverts() {
	ExtBuilder::default().build().execute_with(|| {
		Timestamp::set_timestamp(1_000);
		let sk = permit_test_secret();
		let owner = permit_test_address(&sk);
		let (v, mut r, s) =
			sign_permit(&sk, owner, BOB, 500u64.into(), U256::zero(), 10_000u64.into());
		// Corrupt one byte of `r`.
		r[0] ^= 0xFF;
		precompiles()
			.prepare_test(
				CHARLIE,
				PRECOMPILE_ADDRESS,
				PCall::eip2612_permit {
					owner: Address(owner),
					spender: Address(BOB),
					value: 500u64.into(),
					deadline: 10_000u64.into(),
					v,
					r: H256(r),
					s: H256(s),
				},
			)
			.execute_reverts(|out| out == b"Invalid permit");
	});
}

#[test]
fn permit_expired_deadline_reverts() {
	ExtBuilder::default().build().execute_with(|| {
		// `now_ms / 1000 = 10` > deadline `5` → expired.
		Timestamp::set_timestamp(10_000);
		let sk = permit_test_secret();
		let owner = permit_test_address(&sk);
		let (v, r, s) = sign_permit(&sk, owner, BOB, 500u64.into(), U256::zero(), 5u64.into());
		precompiles()
			.prepare_test(
				CHARLIE,
				PRECOMPILE_ADDRESS,
				PCall::eip2612_permit {
					owner: Address(owner),
					spender: Address(BOB),
					value: 500u64.into(),
					deadline: 5u64.into(),
					v,
					r: H256(r),
					s: H256(s),
				},
			)
			.execute_reverts(|out| out == b"Permit expired");
	});
}

#[test]
fn permit_expired_deadline_millisecond_boundary() {
	ExtBuilder::default().build().execute_with(|| {
		// 1001ms / 1000 = 1, equals deadline 1, NOT greater — allowed.
		Timestamp::set_timestamp(1_001);
		let sk = permit_test_secret();
		let owner = permit_test_address(&sk);
		let (v, r, s) = sign_permit(&sk, owner, BOB, 500u64.into(), U256::zero(), 1u64.into());
		precompiles()
			.prepare_test(
				CHARLIE,
				PRECOMPILE_ADDRESS,
				PCall::eip2612_permit {
					owner: Address(owner),
					spender: Address(BOB),
					value: 500u64.into(),
					deadline: 1u64.into(),
					v,
					r: H256(r),
					s: H256(s),
				},
			)
			.execute_returns(());
	});
}

#[test]
fn check_allowance_not_existing() {
	// Querying allowance for an owner/spender pair that was never set
	// returns zero (no revert).
	ExtBuilder::default().build().execute_with(|| {
		precompiles()
			.prepare_test(
				ALICE,
				PRECOMPILE_ADDRESS,
				PCall::allowance { owner: Address(ALICE), spender: Address(BOB) },
			)
			.execute_returns(U256::zero());
	});
}

// Suppress unused-import warning for `h160` (kept as a generic helper
// in `mock.rs` even though no current test uses it directly).
#[allow(dead_code)]
fn _silence() {
	let _ = h160(0);
}
