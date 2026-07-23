//! Integration across the obj-asteroid to spectral3d seam. The asteroid
//! generator exists to feed spectral3d, so these vectors prove the contract end
//! to end. A seeded body must clear spectral3d's structural gates, land in the
//! well-conditioned shape band its parameters target, and hash to one stable
//! identity every node reproduces.

use std::collections::HashSet;

use obj_asteroid::asteroid;
use spectral3d::{register, scan, verify, SpectralParams};

const SUBDIVISIONS: u32 = 3;
const SEEDS: u64 = 128;

/// Widen a compact vector label to full seed width. Keeps the tables readable
/// without 32 byte literals.
fn seed(label: u64) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&label.to_le_bytes());
    bytes
}

/// Every seeded body clears the structural gates (closed, consistently wound,
/// non-degenerate volume and covariance) and then registers, so it sits inside
/// the well-conditioned band the generator's axis and relief ranges aim for. A
/// rejection here is a real finding about that tuning, not a flaky test. `scan`
/// runs first so a structural failure stays distinct from a shape-gate one.
#[test]
fn every_seed_registers() {
    let p = SpectralParams::default();
    for label in 0..SEEDS {
        if let Err(e) = scan(asteroid(seed(label), SUBDIVISIONS), p.target_samples) {
            panic!("seed {label:#x}: spectral3d rejected the mesh structurally: {e}");
        }
        if let Err(e) = register(asteroid(seed(label), SUBDIVISIONS), &p) {
            panic!("seed {label:#x}: outside the well-conditioned band: {e}");
        }
    }
}

/// One seed, one identity, on every call. Registration is deterministic, and a
/// fresh scan recovers the same hash through the published helper. That is the
/// reproducibility PoScan leans on, a seed pinning its identity bit for bit.
#[test]
fn identity_is_reproducible() {
    let p = SpectralParams::default();
    for label in [0u64, 1, 0x2a, 0xdead_beef] {
        let (id, helper) = register(asteroid(seed(label), SUBDIVISIONS), &p)
            .unwrap_or_else(|e| panic!("seed {label:#x}: register failed: {e}"));
        let (again, _) = register(asteroid(seed(label), SUBDIVISIONS), &p).unwrap();
        assert_eq!(id, again, "seed {label:#x}: identity not reproducible");

        let scanned = verify(asteroid(seed(label), SUBDIVISIONS), &helper, &p).unwrap();
        assert_eq!(scanned, id, "seed {label:#x}: fresh scan did not verify to its identity");
    }
}

/// The seam carries shape variation rather than collapsing every body onto one
/// reading. Tested on the raw `scan` features, where the full entropy lives, not
/// on the registered identity. That identity is a deliberately coarse tag (only
/// ~6 to 15 bits vary across real objects), so two distinct asteroids can share
/// one, by design. Mining entropy rides the scan path, never register.
#[test]
fn the_seam_carries_shape_variation() {
    let p = SpectralParams::default();
    let mut seen = HashSet::new();
    for label in 0..SEEDS {
        let f = scan(asteroid(seed(label), SUBDIVISIONS), p.target_samples)
            .unwrap_or_else(|e| panic!("seed {label:#x}: scan failed: {e}"));
        let bits: Vec<u64> = f.iter().map(|x| x.to_bits()).collect();
        assert!(seen.insert(bits), "seed {label:#x}: identical raw features to an earlier seed");
    }
}

// Cross-environment golden vectors for the seam output. `determinism.rs` freezes
// the upstream mesh, these freeze what actually goes on chain. Run native and in
// wasm, on x86_64 and aarch64 (the determinism CI job). A red vector is a
// consensus break to investigate, never a stale value to refresh.
// `regenerate_golden` is the only sanctioned way to move these.

/// FNV-1a, hand-rolled so a std hasher cannot shift results across
/// toolchains.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Fingerprint the raw IEEE-754 bits of the scan feature vector. Bit exact, so it
/// catches ULP drift before quantization can fold it back into the same bucket.
fn scan_fingerprint(features: &[f64]) -> u64 {
    let mut buf = Vec::new();
    for x in features {
        buf.extend_from_slice(&x.to_bits().to_le_bytes());
    }
    fnv1a(&buf)
}

/// (seed label, scan feature fingerprint). The full-entropy mining path.
const SCAN_GOLDEN: [(u64, u64); 4] = [
    (0x0, 0x0d26821ca134f8ff),
    (0x1, 0xf619359cccd929fa),
    (0x2a, 0x0e72613078377d67),
    (0xdead_beef, 0xf3ca413897081200),
];

/// On-chain mining resolution. The protocol scans a subdivision 4 body at this many
/// samples, so the cross-environment leg must freeze that exact path, not only the
/// lighter default the seam tests share.
const MINING_SUBDIVISIONS: u32 = 4;
const MINING_SAMPLES: usize = 4096;

/// (seed label, scan feature fingerprint) at the on-chain mining resolution.
const SCAN_GOLDEN_MINING: [(u64, u64); 4] = [
    (0x0, 0xb21b2b6986a53da6),
    (0x1, 0xf0d87a7ea9a98f4c),
    (0x2a, 0xf7351ba847a5c29c),
    (0xdead_beef, 0x99e2620e026ad041),
];

/// (seed label, registered identity hash). The coarse on-chain tag.
const IDENTITY_GOLDEN: [(u64, &str); 4] = [
    (0x0, "5a4cff8722d3a2db39090f46f9db2509e5ec3d22a3b4833986b14cd1239996d5"),
    (0x1, "ee01f81ad9bcc11569a8a2a21a10d670114c14aaa670456725f499f66a00b11c"),
    (0x2a, "60f37ddf1f662ecaeb72a0c6b2e4122ab146ff8f996447ea1d30253116383269"),
    (0xdead_beef, "87f04183be78b25602d9e30f5d973499bb2ee9e507aab2fe2ff59c2c5761bd6f"),
];

#[test]
fn scan_golden_vectors() {
    let p = SpectralParams::default();
    for (label, want) in SCAN_GOLDEN {
        let f = scan(asteroid(seed(label), SUBDIVISIONS), p.target_samples)
            .unwrap_or_else(|e| panic!("seed {label:#x}: scan failed: {e}"));
        let got = scan_fingerprint(&f);
        assert_eq!(got, want, "seed={label:#x}: scan features drifted, got 0x{got:016x}");
    }
}

#[test]
fn scan_golden_vectors_mining() {
    for (label, want) in SCAN_GOLDEN_MINING {
        let f = scan(asteroid(seed(label), MINING_SUBDIVISIONS), MINING_SAMPLES)
            .unwrap_or_else(|e| panic!("seed {label:#x}: scan failed: {e}"));
        let got = scan_fingerprint(&f);
        assert_eq!(got, want, "seed={label:#x}: mining-resolution scan drifted, got 0x{got:016x}");
    }
}

#[test]
fn identity_golden_vectors() {
    let p = SpectralParams::default();
    for (label, want) in IDENTITY_GOLDEN {
        let (id, _) = register(asteroid(seed(label), SUBDIVISIONS), &p)
            .unwrap_or_else(|e| panic!("seed {label:#x}: register failed: {e}"));
        assert_eq!(id, want, "seed={label:#x}: identity drifted, got {id}");
    }
}

/// Print current golden values. Run ONLY after a deliberate decision to move the
/// canonical shape, then paste the values above. Refreshing it to silence a red
/// vector hands the chain a silent fork.
///   cargo test -p obj-asteroid --test spectral -- --ignored --nocapture regenerate_golden
#[test]
#[ignore]
fn regenerate_golden() {
    let p = SpectralParams::default();
    for (label, _) in SCAN_GOLDEN {
        let f = scan(asteroid(seed(label), SUBDIVISIONS), p.target_samples).unwrap();
        println!("scan  ({label:#x}, 0x{:016x}),", scan_fingerprint(&f));
    }
    for (label, _) in SCAN_GOLDEN_MINING {
        let f = scan(asteroid(seed(label), MINING_SUBDIVISIONS), MINING_SAMPLES).unwrap();
        println!("mine  ({label:#x}, 0x{:016x}),", scan_fingerprint(&f));
    }
    for (label, _) in IDENTITY_GOLDEN {
        let (id, _) = register(asteroid(seed(label), SUBDIVISIONS), &p).unwrap();
        println!("ident ({label:#x}, {id:?}),");
    }
}
