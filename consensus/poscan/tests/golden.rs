//! Golden vectors for the consensus work value `s`. `Compute::work` grows the
//! asteroid, scans it at the on-chain resolution, fine quantizes the features and
//! hashes the buckets. One input must reproduce one `s` on every node or the chain
//! forks. These freeze `s` bit-for-bit.
//!
//! The vectors run native on x86_64 and aarch64 and under wasm32-wasip1 via
//! wasmtime, so any target that reorders the float path turns them red. Running
//! `s` itself under wasm freezes the consensus value on the wasm float path the
//! runtime verifies on, with no transitive proxy.
//!
//! A red vector is a consensus break to investigate, never a stale value to
//! refresh. `regenerate` is the only sanctioned way to move them.

use poscan::Compute;
use primitive_types::{H256, U256};

/// (pre_hash low u64, nonce, lowercase hex of `s`). The value that goes on chain.
const WORK_GOLDEN: [(u64, u64, &str); 4] = [
	(0x0, 0x0, "5cea70c24b6ff7d7761e8817f63e1068baeeabb132fb08c918159127243bac8d"),
	(0x1, 0x1, "864d75aa5b4599aa6cf29a8c76681c800d8177efd0a209c492324b4a84eb6961"),
	(0x2a, 0x2a, "0f155bccfa1c57143c04e47f9affec27a84a828e75da5fd4ff8422919c6649b6"),
	(0xdead_beef, 0xdead_beef, "55657c14aefeb8dffd90838e78dbc059f152cdff13ba3d280acccaaf44fcba2c"),
];

fn work_of(pre: u64, nonce: u64) -> H256 {
	Compute { pre_hash: H256::from_low_u64_be(pre), nonce: U256::from(nonce) }
		.work()
		.unwrap_or_else(|| panic!("pre={pre:#x} nonce={nonce:#x}: mesh was unscannable"))
}

#[test]
fn work_golden_vectors() {
	for (pre, nonce, want) in WORK_GOLDEN {
		let got = format!("{:x}", work_of(pre, nonce));
		assert_eq!(got, want, "pre={pre:#x} nonce={nonce:#x}: work value drifted, got {got}");
	}
}

#[test]
fn work_is_reproducible() {
	assert_eq!(work_of(7, 7), work_of(7, 7));
}

/// Print current values. Run ONLY after a deliberate decision to move the canonical
/// `s`, then paste the values above. Running it to silence a red vector hands the
/// chain a silent fork.
///   cargo test -p poscan --test golden -- --ignored --nocapture regenerate
#[test]
#[ignore]
fn regenerate() {
	for (pre, nonce, _) in WORK_GOLDEN {
		println!("({pre:#x}, {nonce:#x}, \"{:x}\"),", work_of(pre, nonce));
	}
}
