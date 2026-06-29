//! Golden determinism vectors. The asteroid pipeline is consensus-critical. One seed
//! must reproduce one mesh, bit-for-bit, on every node, or PoScan's `s` forks. These
//! fingerprints freeze the exact f64 output of fixed seeds. A mismatch
//! means a float path or a dependency (rand_pcg, libm) shifted the canonical
//! shape. Treat a red test as a consensus break to investigate, never a stale value
//! to refresh. `regenerate` is the only sanctioned way to move these.

use obj_asteroid::asteroid;
use obj_asteroid::Mesh;

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

/// (seed, subdivisions, mesh fingerprint). Subdivision 4 is the on-chain mining
/// resolution, frozen here beside the lighter level the seam tests exercise.
const MESH_GOLDEN: [(u64, u32, u64); 8] = [
    (0x0, 3, 0xb192d0954cf2ad83),
    (0x1, 3, 0xf5c3672f25b02031),
    (0x2a, 3, 0xa4f06fe37253ce42),
    (0xdead_beef, 3, 0xa0b0d08c5deafe5c),
    (0x0, 4, 0x8291dee3ea07baa0),
    (0x1, 4, 0x56c2a257a4f6cb55),
    (0x2a, 4, 0x0c78e63b5245717d),
    (0xdead_beef, 4, 0x2547477c49d8ea72),
];

#[test]
fn mesh_golden_vectors() {
    for (seed, sub, want) in MESH_GOLDEN {
        let got = mesh_fingerprint(&asteroid(seed, sub));
        assert_eq!(got, want, "seed={seed:#x} sub={sub}: mesh drifted, got 0x{got:016x}");
    }
}

/// A seed must land on the same mesh every call. Guards against nondeterminism a
/// single-shot golden could only catch by flaking, e.g. swapping an ordered map for a
/// hashed one.
#[test]
fn reproducible() {
    let a = asteroid(7, 3);
    let b = asteroid(7, 3);
    assert_eq!(mesh_fingerprint(&a), mesh_fingerprint(&b));
}

/// Print current fingerprints. Run ONLY after a deliberate decision to change the
/// canonical shape, then paste the values above. Running it to silence a red golden
/// hands the chain a silent fork.
///   cargo test -p obj-asteroid --test determinism -- --ignored --nocapture
#[test]
#[ignore]
fn regenerate() {
    for (seed, sub, _) in MESH_GOLDEN {
        println!("({seed:#x}, {sub}, 0x{:016x}),", mesh_fingerprint(&asteroid(seed, sub)));
    }
}
