#![no_std]

extern crate alloc;

use alloc::vec::Vec;

pub mod shape;

mod icosphere;
mod perlin;
mod rng;

pub use spectral3d::Mesh;

use crate::icosphere::icosphere;
use crate::rng::Rng;
use shape::{AsteroidParams, Lobe};

pub const OCTAVES: usize = 5;
pub const NUM_CRATERS: usize = 10;
pub const NUM_LOBES: usize = 4;

/// Grow one asteroid from a seed. Shape params derive from the seed, so the same
/// seed always reproduces the same mesh.
pub fn asteroid(seed: [u8; 32], subdivisions: u32) -> Mesh {
    let base = icosphere(subdivisions);

    let mut rng = Rng::new(seed);

    // Wide axis spread covers the Vesta/Kleopatra/67P aspect-ratio band and feeds
    // spectral3d's eigenvalue-ratio dimensions (lam21/lam31). Bias toward elongated
    // shapes with the middle axis near the short one, away from a flat disk.
    let lam31 = rng.range(0.18, 0.90); // shortest²/longest² axis ratio, strongly elongated to near-spherical
    let lam21 = rng.range(lam31, (1.0 + lam31) * 0.5); // middle axis biased short, cigar not disk
    let mut axes = [1.0, libm::sqrt(lam21), libm::sqrt(lam31)];
    shuffle3(&mut axes, &mut rng); // shuffle axis assignment, drop the "longest axis is always x" bias

    // Broad gentle directional swells. Each lobe stays soft with sharp in 1..2
    // (lower reads rounder and chubbier), small amplitude, and a positive bias.
    // They build large-scale asymmetry without poking spikes, yielding a slightly
    // lopsided rock rather than a sea urchin growing spines.
    let mut lobes = Vec::with_capacity(NUM_LOBES);
    for _ in 0..NUM_LOBES {
        lobes.push(Lobe {
            dir: rng.unit_vector(),
            amp: rng.range(0.08, 0.20),
            sharp: rng.range(1.0, 2.0),
            sign: if rng.unit() < 0.25 { -1.0 } else { 1.0 },
        });
    }

    let params = AsteroidParams {
        noise_amplitude: rng.range(0.16, 0.30),
        noise_frequency: rng.range(1.1, 2.4),
        octaves: OCTAVES,
        num_craters: NUM_CRATERS,
        axis_scale: axes,
        lobes,
        ..AsteroidParams::default()
    };

    shape::sculpt(base, &params, &mut rng)
}

/// Fisher-Yates shuffle over 3 elements.
fn shuffle3(a: &mut [f64; 3], rng: &mut Rng) {
    for i in (1..3).rev() {
        let j = ((rng.unit() * (i as f64 + 1.0)) as usize).min(i);
        a.swap(i, j);
    }
}
