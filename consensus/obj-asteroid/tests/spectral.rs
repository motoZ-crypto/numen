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

/// Every seeded body clears the structural gates (closed, consistently wound,
/// non-degenerate volume and covariance) and then registers, so it sits inside
/// the well-conditioned band the generator's axis and relief ranges aim for. A
/// rejection here is a real finding about that tuning, not a flaky test. `scan`
/// runs first so a structural failure stays distinct from a shape-gate one.
#[test]
fn every_seed_registers() {
    let p = SpectralParams::default();
    for seed in 0..SEEDS {
        if let Err(e) = scan(asteroid(seed, SUBDIVISIONS), p.target_samples) {
            panic!("seed {seed:#x}: spectral3d rejected the mesh structurally: {e}");
        }
        if let Err(e) = register(asteroid(seed, SUBDIVISIONS), &p) {
            panic!("seed {seed:#x}: outside the well-conditioned band: {e}");
        }
    }
}

/// One seed, one identity, on every call. Registration is deterministic, and a
/// fresh scan recovers the same hash through the published helper. That is the
/// reproducibility PoScan leans on, a seed pinning its identity bit for bit.
#[test]
fn identity_is_reproducible() {
    let p = SpectralParams::default();
    for seed in [0u64, 1, 0x2a, 0xdead_beef] {
        let (id, helper) = register(asteroid(seed, SUBDIVISIONS), &p)
            .unwrap_or_else(|e| panic!("seed {seed:#x}: register failed: {e}"));
        let (again, _) = register(asteroid(seed, SUBDIVISIONS), &p).unwrap();
        assert_eq!(id, again, "seed {seed:#x}: identity not reproducible");

        let scanned = verify(asteroid(seed, SUBDIVISIONS), &helper, &p).unwrap();
        assert_eq!(scanned, id, "seed {seed:#x}: fresh scan did not verify to its identity");
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
    for seed in 0..SEEDS {
        let f = scan(asteroid(seed, SUBDIVISIONS), p.target_samples)
            .unwrap_or_else(|e| panic!("seed {seed:#x}: scan failed: {e}"));
        let bits: Vec<u64> = f.iter().map(|x| x.to_bits()).collect();
        assert!(seen.insert(bits), "seed {seed:#x}: identical raw features to an earlier seed");
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

/// (seed, scan feature fingerprint). The full-entropy mining path.
const SCAN_GOLDEN: [(u64, u64); 4] = [
    (0x0, 0xd273571a63c4048b),
    (0x1, 0xb658101f364623e9),
    (0x2a, 0xa1187161662d5331),
    (0xdead_beef, 0x37e165355dfcda75),
];

/// On-chain mining resolution. The protocol scans a subdivision 4 body at this many
/// samples, so the cross-environment leg must freeze that exact path, not only the
/// lighter default the seam tests share.
const MINING_SUBDIVISIONS: u32 = 4;
const MINING_SAMPLES: usize = 4096;

/// (seed, scan feature fingerprint) at the on-chain mining resolution.
const SCAN_GOLDEN_MINING: [(u64, u64); 4] = [
    (0x0, 0x6da1b2a0b181616c),
    (0x1, 0xd3e887809042186c),
    (0x2a, 0x88be365da70385d6),
    (0xdead_beef, 0xbbb64605ce4c62cc),
];

/// (seed, registered identity hash). The coarse on-chain tag.
const IDENTITY_GOLDEN: [(u64, &str); 4] = [
    (0x0, "1f8e42839dbd9894eca3c17c1140d85c6513eee5b14c4e7e3e766b29b9310f63"),
    (0x1, "5e814cb3706b947ff65a461d8df0c6463275067ab9624b0fd799c302c227210f"),
    (0x2a, "d28b296abd9bccb34c954373a154aff5f8c2cf1b0784f6d160ae5ba4060718f5"),
    (0xdead_beef, "e6c741b0da6ee895054ea0daa59243a097bd3e41767f494a90dfb68b2e57096d"),
];

#[test]
fn scan_golden_vectors() {
    let p = SpectralParams::default();
    for (seed, want) in SCAN_GOLDEN {
        let f = scan(asteroid(seed, SUBDIVISIONS), p.target_samples)
            .unwrap_or_else(|e| panic!("seed {seed:#x}: scan failed: {e}"));
        let got = scan_fingerprint(&f);
        assert_eq!(got, want, "seed={seed:#x}: scan features drifted, got 0x{got:016x}");
    }
}

#[test]
fn scan_golden_vectors_mining() {
    for (seed, want) in SCAN_GOLDEN_MINING {
        let f = scan(asteroid(seed, MINING_SUBDIVISIONS), MINING_SAMPLES)
            .unwrap_or_else(|e| panic!("seed {seed:#x}: scan failed: {e}"));
        let got = scan_fingerprint(&f);
        assert_eq!(got, want, "seed={seed:#x}: mining-resolution scan drifted, got 0x{got:016x}");
    }
}

#[test]
fn identity_golden_vectors() {
    let p = SpectralParams::default();
    for (seed, want) in IDENTITY_GOLDEN {
        let (id, _) = register(asteroid(seed, SUBDIVISIONS), &p)
            .unwrap_or_else(|e| panic!("seed {seed:#x}: register failed: {e}"));
        assert_eq!(id, want, "seed={seed:#x}: identity drifted, got {id}");
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
    for (seed, _) in SCAN_GOLDEN {
        let f = scan(asteroid(seed, SUBDIVISIONS), p.target_samples).unwrap();
        println!("scan  ({seed:#x}, 0x{:016x}),", scan_fingerprint(&f));
    }
    for (seed, _) in SCAN_GOLDEN_MINING {
        let f = scan(asteroid(seed, MINING_SUBDIVISIONS), MINING_SAMPLES).unwrap();
        println!("mine  ({seed:#x}, 0x{:016x}),", scan_fingerprint(&f));
    }
    for (seed, _) in IDENTITY_GOLDEN {
        let (id, _) = register(asteroid(seed, SUBDIVISIONS), &p).unwrap();
        println!("ident ({seed:#x}, {id:?}),");
    }
}
