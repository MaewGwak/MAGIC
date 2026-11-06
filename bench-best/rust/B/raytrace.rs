// raytrace 1536^2 -> 336141.941
//
// Same 5-sphere scene and per-pixel formula as the reference, with two
// standard raytracer optimisations that leave the value untouched:
//
//  1. Bounding-volume culling.  For a sphere, disc > 0 is a quadratic in the
//     screen coordinate u, so each scanline has an explicit integer x-range
//     where that sphere can possibly be hit.  Only x positions covered by at
//     least one sphere's range are visited at all; every other pixel has
//     col = 0 and adding it would not change the f64 sum.  The ranges are
//     expanded by one pixel, so no borderline pixel is lost.
//  2. Threads.  The 1536 scanlines are rendered by 16 workers; each row keeps
//     its own partial sum and the rows are added in ascending order, making
//     the reduction deterministic.
//
// The reference's unrounded total is 336141.9411024, which is 4.0e-4 from the
// next 3-decimal rounding boundary, so the 1e-9-relative check is satisfied
// even though this version's summation groups differ in the last bits.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;

const W: usize = 1536;
const H: usize = 1536;
const HW: f64 = 768.0;
const INV_HW: f64 = 1.0 / 768.0;

const CX: [f64; 5] = [0.0, 1.5, -1.2, 2.5, -2.0];
const CY: [f64; 5] = [0.0, 0.3, -0.6, -0.8, 1.2];
const CZ: [f64; 5] = [3.0, 3.2, 2.8, 4.0, 3.5];
const RR: [f64; 5] = [1.0, 0.6, 0.7, 0.5, 0.4];
const LX: f64 = 5.0;
const LY: f64 = 5.0;
const LZ: f64 = 0.0;

// c_s = |centre|^2 - r^2, in the reference's evaluation order.
// 1/|p-C| = 1/r for a point on the sphere; using the exact radius saves one
// sqrt per hit and is numerically identical to the reference to ~1e-16.
const INV_RR: [f64; 5] = [1.0 / 1.0, 1.0 / 0.6, 1.0 / 0.7, 1.0 / 0.5, 1.0 / 0.4];

const CC: [f64; 5] = [
    CX[0] * CX[0] + CY[0] * CY[0] + CZ[0] * CZ[0] - RR[0] * RR[0],
    CX[1] * CX[1] + CY[1] * CY[1] + CZ[1] * CZ[1] - RR[1] * RR[1],
    CX[2] * CX[2] + CY[2] * CY[2] + CZ[2] * CZ[2] - RR[2] * RR[2],
    CX[3] * CX[3] + CY[3] * CY[3] + CZ[3] * CZ[3] - RR[3] * RR[3],
    CX[4] * CX[4] + CY[4] * CY[4] + CZ[4] * CZ[4] - RR[4] * RR[4],
];

#[inline]
fn render_row(y: usize) -> f64 {
    let v = (y as f64 - HW) * INV_HW;
    let base = v * v + 2.25; // v^2 + dz^2, dz = 1.5

    // Integer x range in which each sphere can satisfy disc > 0.
    let mut lo = [1i32; 5];
    let mut hi = [0i32; 5];
    let mut nint = 0usize;
    let mut ints = [(0i32, 0i32); 5];

    let mut s = 0usize;
    while s < 5 {
        let k = CY[s] * v + CZ[s] * 1.5;
        let a = CX[s] * CX[s] - CC[s];
        if a >= 0.0 {
            // Conservative fallback: the projected region is not bounded
            // between two roots (never happens for this scene).
            lo[s] = 0;
            hi[s] = W as i32 - 1;
            ints[nint] = (0, W as i32 - 1);
            nint += 1;
        } else {
            let bq = 2.0 * CX[s] * k;
            let cq = k * k - CC[s] * base;
            let dq = bq * bq - 4.0 * a * cq;
            if dq > 0.0 {
                let sq = dq.sqrt();
                let inv2a = 1.0 / (2.0 * a);
                let mut u1 = (-bq - sq) * inv2a;
                let mut u2 = (-bq + sq) * inv2a;
                if u1 > u2 {
                    core::mem::swap(&mut u1, &mut u2);
                }
                // Expand one pixel each way and clamp.
                let xl = ((u1 * HW + HW).floor() as i32 - 1).max(0);
                let xh = ((u2 * HW + HW).ceil() as i32 + 1).min(W as i32 - 1);
                if xl <= xh {
                    lo[s] = xl;
                    hi[s] = xh;
                    ints[nint] = (xl, xh);
                    nint += 1;
                }
            }
        }
        s += 1;
    }

    if nint == 0 {
        return 0.0;
    }

    // Sort the (at most 5) intervals by left edge, then merge them.
    let mut i = 1usize;
    while i < nint {
        let cur = ints[i];
        let mut j = i;
        while j > 0 && ints[j - 1].0 > cur.0 {
            ints[j] = ints[j - 1];
            j -= 1;
        }
        ints[j] = cur;
        i += 1;
    }
    let mut merged = [(0i32, 0i32); 5];
    let mut nm = 1usize;
    merged[0] = ints[0];
    let mut q = 1usize;
    while q < nint {
        let last = nm - 1;
        if ints[q].0 <= merged[last].1 + 1 {
            if ints[q].1 > merged[last].1 {
                merged[last].1 = ints[q].1;
            }
        } else {
            merged[nm] = ints[q];
            nm += 1;
        }
        q += 1;
    }

    let mut rowsum = 0.0f64;
    let mut r = 0usize;
    while r < nm {
        let (x0, x1) = merged[r];
        let mut x = x0 as usize;
        while x <= x1 as usize {
            let u = (x as f64 - HW) * INV_HW;
            let len = (u * u + base).sqrt();
            let inv = 1.0 / len;
            let dx = u * inv;
            let dy = v * inv;
            let dz = 1.5 * inv;

            let mut best = 1.0e9f64;
            let mut hit: i32 = -1;
            let xi = x as i32;

            let mut t = 0usize;
            while t < 5 {
                if xi >= lo[t] && xi <= hi[t] {
                    let b = -(CX[t] * dx + CY[t] * dy + CZ[t] * dz);
                    let disc = b * b - CC[t];
                    if disc > 0.0 {
                        let tt = -b - disc.sqrt();
                        if tt > 0.0 && tt < best {
                            best = tt;
                            hit = t as i32;
                        }
                    }
                }
                t += 1;
            }

            if hit >= 0 {
                let si = hit as usize;
                let px = dx * best;
                let py = dy * best;
                let pz = dz * best;
                let nx = px - CX[si];
                let ny = py - CY[si];
                let nz = pz - CZ[si];
                let invnl = INV_RR[si];
                let ldx = LX - px;
                let ldy = LY - py;
                let ldz = LZ - pz;
                let ll = (ldx * ldx + ldy * ldy + ldz * ldz).sqrt();
                let mut d = ((nx * invnl) * ldx + (ny * invnl) * ldy + (nz * invnl) * ldz) / ll;
                if d < 0.0 {
                    d = 0.0;
                }
                rowsum += d;
            }

            x += 1;
        }
        r += 1;
    }
    rowsum
}

fn main() {
    let nthreads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(H);

    let rows: Vec<AtomicU64> = (0..H).map(|_| AtomicU64::new(0)).collect();
    let next = AtomicUsize::new(0);

    thread::scope(|scope| {
        for _ in 0..nthreads {
            scope.spawn(|| loop {
                let y = next.fetch_add(1, Ordering::Relaxed);
                if y >= H {
                    break;
                }
                rows[y].store(render_row(y).to_bits(), Ordering::Relaxed);
            });
        }
    });

    let total: f64 = (0..H)
        .map(|y| f64::from_bits(rows[y].load(Ordering::Relaxed)))
        .sum();
    println!("{:.3}", total);
}
