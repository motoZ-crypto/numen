//! Golden determinism vectors. The asteroid pipeline is consensus-critical. One seed
//! must reproduce one mesh, bit-for-bit, on every node, or PoScan's `s` forks. These
//! fingerprints freeze the exact f64 output of fixed seeds. A mismatch
//! means a float path or a dependency (rand_pcg, libm) shifted the canonical
//! shape. Treat a red test as a consensus break to investigate, never a stale value
//! to refresh. `regenerate` is the only sanctioned way to move these.

use obj_asteroid::asteroid;
use obj_asteroid::Mesh;

/// Widen a compact vector label to full seed width. Keeps the tables readable
/// without 32 byte literals.
fn seed(label: u64) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&label.to_le_bytes());
    bytes
}

/// FNV-1a over a byte stream, self-contained on purpose. A std hasher's output can
/// drift across toolchains and move the goalposts under us.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Fingerprint the raw IEEE-754 bits of every coordinate plus the face indices. Bit
/// exact, so it catches even ULP drift in the mesh handed to spectral3d.
fn mesh_fingerprint(mesh: &Mesh) -> u64 {
    let mut buf = Vec::new();
    for v in &mesh.vertices {
        for c in v {
            buf.extend_from_slice(&c.to_bits().to_le_bytes());
        }
    }
    for f in &mesh.faces {
        for idx in f {
            buf.extend_from_slice(&idx.to_le_bytes());
        }
    }
    fnv1a(&buf)
}

/// (seed label, subdivisions, mesh fingerprint). Subdivision 4 is the on-chain mining
/// resolution, frozen here beside the lighter level the seam tests exercise.
const MESH_GOLDEN: [(u64, u32, u64); 8] = [
    (0x0, 3, 0x60eece03403261ce),
    (0x1, 3, 0x63f00c5d8eea8791),
    (0x2a, 3, 0xbfc876e62d6cef68),
    (0xdead_beef, 3, 0x1ceba8fc054c6bb8),
    (0x0, 4, 0x25dd32a1cc684937),
    (0x1, 4, 0x0f6892f5698f0014),
    (0x2a, 4, 0xcdccf22dc20a0aa8),
    (0xdead_beef, 4, 0x5cf92daacd522599),
];

#[test]
fn mesh_golden_vectors() {
    for (label, sub, want) in MESH_GOLDEN {
        let got = mesh_fingerprint(&asteroid(seed(label), sub));
        assert_eq!(got, want, "seed={label:#x} sub={sub}: mesh drifted, got 0x{got:016x}");
    }
}

/// Two seeds differing only outside the low 8 bytes must grow different bodies. A
/// generator that folds the seed down to a narrow word collapses them onto one mesh.
/// That caps the work domain at the narrow width, which puts it in reach of an
/// offline scan.
#[test]
fn the_whole_seed_reaches_the_mesh() {
    let mut low = [0u8; 32];
    low[0] = 1;
    let mut high = low;
    high[31] = 1;
    assert_ne!(mesh_fingerprint(&asteroid(low, 3)), mesh_fingerprint(&asteroid(high, 3)));
}

/// A seed must land on the same mesh every call. Guards against nondeterminism a
/// single-shot golden could only catch by flaking, e.g. swapping an ordered map for a
/// hashed one.
#[test]
fn reproducible() {
    let a = asteroid(seed(7), 3);
    let b = asteroid(seed(7), 3);
    assert_eq!(mesh_fingerprint(&a), mesh_fingerprint(&b));
}

/// Print current fingerprints. Run ONLY after a deliberate decision to change the
/// canonical shape, then paste the values above. Running it to silence a red golden
/// hands the chain a silent fork.
///   cargo test -p obj-asteroid --test determinism -- --ignored --nocapture
#[test]
#[ignore]
fn regenerate() {
    for (label, sub, _) in MESH_GOLDEN {
        println!("({label:#x}, {sub}, 0x{:016x}),", mesh_fingerprint(&asteroid(seed(label), sub)));
    }
}
