//! Perlin fBm relief, frozen for consensus. Classic 3D gradient noise with a quintic
//! fade, summed over octaves into fractal Brownian motion. Every op is mul/add/floor
//! routed through libm, so the field is bit-identical on every target. A native miner
//! and the on-chain wasm runtime agree.

use crate::rng::Rng;

const LACUNARITY: f64 = 2.0;
const PERSISTENCE: f64 = 0.5;

/// Fractal Brownian motion over seeded Perlin noise.
pub struct Fbm {
    perm: [u8; 512],
    octaves: usize,
    frequency: f64,
}

impl Fbm {
    /// The seed drives a Fisher-Yates shuffle of the permutation table, so
    /// neighbouring seeds give decorrelated fields, not a shifted copy of one.
    pub fn new(seed: [u8; 32], octaves: usize, frequency: f64) -> Self {
        Fbm {
            perm: shuffled_perm(seed),
            octaves,
            frequency,
        }
    }

    /// Sample the fractal sum. Output settles near [-1, 1]. Octaves are normalized by
    /// their summed amplitude, so the range holds regardless of octave count.
    pub fn get(&self, point: [f64; 3]) -> f64 {
        let mut freq = self.frequency;
        let mut amp = 1.0;
        let mut sum = 0.0;
        let mut norm = 0.0;
        for _ in 0..self.octaves {
            sum += amp * self.noise(point[0] * freq, point[1] * freq, point[2] * freq);
            norm += amp;
            freq *= LACUNARITY;
            amp *= PERSISTENCE;
        }
        if norm > 0.0 {
            sum / norm
        } else {
            0.0
        }
    }

    /// One octave of classic 3D Perlin noise, a trilinear blend of eight corner
    /// gradients, eased by the quintic fade.
    fn noise(&self, x: f64, y: f64, z: f64) -> f64 {
        let p = &self.perm;

        let xi = (libm::floor(x) as i32 & 255) as usize;
        let yi = (libm::floor(y) as i32 & 255) as usize;
        let zi = (libm::floor(z) as i32 & 255) as usize;

        let x = x - libm::floor(x);
        let y = y - libm::floor(y);
        let z = z - libm::floor(z);

        let u = fade(x);
        let v = fade(y);
        let w = fade(z);

        let a  = p[xi    ] as usize + yi;
        let aa = p[a     ] as usize + zi;
        let ab = p[a + 1 ] as usize + zi;
        let b  = p[xi + 1] as usize + yi;
        let ba = p[b     ] as usize + zi;
        let bb = p[b + 1 ] as usize + zi;

        lerp(
            w,
            lerp(
                v,
                lerp(u, grad(p[aa], x, y, z), grad(p[ba], x - 1.0, y, z)),
                lerp(u, grad(p[ab], x, y - 1.0, z), grad(p[bb], x - 1.0, y - 1.0, z)),
            ),
            lerp(
                v,
                lerp(
                    u,
                    grad(p[aa + 1], x, y, z - 1.0),
                    grad(p[ba + 1], x - 1.0, y, z - 1.0),
                ),
                lerp(
                    u,
                    grad(p[ab + 1], x, y - 1.0, z - 1.0),
                    grad(p[bb + 1], x - 1.0, y - 1.0, z - 1.0),
                ),
            ),
        )
    }
}

/// A seed-specific permutation of 0..256, mirrored into 512 so corner lookups never
/// need a wrap-around mask.
fn shuffled_perm(seed: [u8; 32]) -> [u8; 512] {
    let mut rng = Rng::new(seed);
    let mut p = [0u8; 256];
    for (i, slot) in p.iter_mut().enumerate() {
        *slot = i as u8;
    }
    for i in (1..256).rev() {
        let j = ((rng.unit() * (i as f64 + 1.0)) as usize).min(i);
        p.swap(i, j);
    }

    let mut perm = [0u8; 512];
    perm[..256].copy_from_slice(&p);
    perm[256..].copy_from_slice(&p);
    perm
}

/// Quintic ease curve 6t^5 - 15t^4 + 10t^3, zero first and second derivatives at the
/// endpoints, so cells join without creases.
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

/// Dot the corner's pseudo-random gradient (one of twelve, picked by the low hash
/// bits) with the distance vector.
fn grad(hash: u8, x: f64, y: f64, z: f64) -> f64 {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        z
    };
    (if (h & 1) == 0 { u } else { -u }) + (if (h & 2) == 0 { v } else { -v })
}
