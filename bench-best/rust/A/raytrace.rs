// Ray tracer, 1536x1536, 5 spheres, f64.  Output: 336141.941.
//
// Parallel over scanlines; each scanline is traced four pixels at a time with
// AVX2 f64 vectors.  Sphere intersections are branch-free (compare/blend), and
// the shading pass gathers the hit sphere's centre with an AVX2 gather.  Each
// thread owns a private vector accumulator; the final sum is the sum of the
// per-thread partials (a different addition order than the scalar reference,
// which changes the result by O(1e-15) relative -- far inside the 1e-9 bound).
use std::arch::x86_64::*;

const W: usize = 1536;
const H: usize = 1536;

const CX: [f64; 5] = [0.0, 1.5, -1.2, 2.5, -2.0];
const CY: [f64; 5] = [0.0, 0.3, -0.6, -0.8, 1.2];
const CZ: [f64; 5] = [3.0, 3.2, 2.8, 4.0, 3.5];
const RR: [f64; 5] = [1.0, 0.6, 0.7, 0.5, 0.4];
const LX: f64 = 5.0;
const LY: f64 = 5.0;
const LZ: f64 = 0.0;

#[target_feature(enable = "avx2")]
unsafe fn trace(y0: usize, y1: usize) -> f64 {
    let zero = _mm256_setzero_pd();
    let mut acc = zero;
    let inv_w = 1.0 / 768.0;
    for y in y0..y1 {
        let v = (y as f64 - 768.0) / 768.0;
        let dz = 1.5f64;
        let dy2z2 = v * v + dz * dz;
        let mut u = (0.0 - 768.0) * inv_w;
        let du = inv_w;
        let mut x = 0usize;
        while x < W {
            let dxv = _mm256_set_pd(u + 3.0 * du, u + 2.0 * du, u + du, u);
            let len = _mm256_sqrt_pd(_mm256_add_pd(_mm256_mul_pd(dxv, dxv), _mm256_set1_pd(dy2z2)));
            let inv = _mm256_div_pd(_mm256_set1_pd(1.0), len);
            let dxv = _mm256_mul_pd(dxv, inv);
            let dyv = _mm256_mul_pd(_mm256_set1_pd(v), inv);
            let dzv = _mm256_mul_pd(_mm256_set1_pd(dz), inv);

            let mut best = _mm256_set1_pd(1_000_000_000.0);
            let mut hit = _mm256_set1_pd(-1.0);
            for s in 0..5 {
                let ox = -CX[s];
                let oy = -CY[s];
                let oz = -CZ[s];
                let b = _mm256_add_pd(
                    _mm256_mul_pd(_mm256_set1_pd(ox), dxv),
                    _mm256_add_pd(
                        _mm256_mul_pd(_mm256_set1_pd(oy), dyv),
                        _mm256_mul_pd(_mm256_set1_pd(oz), dzv),
                    ),
                );
                let c0 = ox * ox + oy * oy + oz * oz - RR[s] * RR[s];
                let disc = _mm256_sub_pd(_mm256_mul_pd(b, b), _mm256_set1_pd(c0));
                let t = _mm256_sub_pd(_mm256_sub_pd(zero, b), _mm256_sqrt_pd(disc));
                let mut m = _mm256_cmp_pd::<_CMP_GT_OQ>(disc, zero);
                m = _mm256_and_pd(m, _mm256_cmp_pd::<_CMP_GT_OQ>(t, zero));
                m = _mm256_and_pd(m, _mm256_cmp_pd::<_CMP_LT_OQ>(t, best));
                best = _mm256_blendv_pd(best, t, m);
                hit = _mm256_blendv_pd(hit, _mm256_set1_pd(s as f64), m);
            }

            // Shading for hit pixels.  idx = clamp(hit,0,4) -> gather centres.
            let hit_safe = _mm256_max_pd(hit, zero);
            let idx32 = _mm256_cvttpd_epi32(hit_safe);
            let idx64 = _mm256_cvtepi32_epi64(idx32);
            let px = _mm256_mul_pd(dxv, best);
            let py = _mm256_mul_pd(dyv, best);
            let pz = _mm256_mul_pd(dzv, best);
            let gx = _mm256_i64gather_pd(CX.as_ptr(), idx64, 8);
            let gy = _mm256_i64gather_pd(CY.as_ptr(), idx64, 8);
            let gz = _mm256_i64gather_pd(CZ.as_ptr(), idx64, 8);
            let mut nx = _mm256_sub_pd(px, gx);
            let mut ny = _mm256_sub_pd(py, gy);
            let mut nz = _mm256_sub_pd(pz, gz);
            let nl = _mm256_sqrt_pd(_mm256_add_pd(
                _mm256_mul_pd(nx, nx),
                _mm256_add_pd(_mm256_mul_pd(ny, ny), _mm256_mul_pd(nz, nz)),
            ));
            let nli = _mm256_div_pd(_mm256_set1_pd(1.0), nl);
            nx = _mm256_mul_pd(nx, nli);
            ny = _mm256_mul_pd(ny, nli);
            nz = _mm256_mul_pd(nz, nli);
            let ldx = _mm256_sub_pd(_mm256_set1_pd(LX), px);
            let ldy = _mm256_sub_pd(_mm256_set1_pd(LY), py);
            let ldz = _mm256_sub_pd(_mm256_set1_pd(LZ), pz);
            let ll = _mm256_sqrt_pd(_mm256_add_pd(
                _mm256_mul_pd(ldx, ldx),
                _mm256_add_pd(_mm256_mul_pd(ldy, ldy), _mm256_mul_pd(ldz, ldz)),
            ));
            let mut d = _mm256_div_pd(
                _mm256_add_pd(
                    _mm256_mul_pd(nx, ldx),
                    _mm256_add_pd(_mm256_mul_pd(ny, ldy), _mm256_mul_pd(nz, ldz)),
                ),
                ll,
            );
            d = _mm256_max_pd(d, zero);
            let col = _mm256_and_pd(d, _mm256_cmp_pd::<_CMP_GE_OQ>(hit, zero));
            acc = _mm256_add_pd(acc, col);

            u += 4.0 * du;
            x += 4;
        }
    }
    let mut lanes = [0f64; 4];
    _mm256_storeu_pd(lanes.as_mut_ptr(), acc);
    lanes[0] + lanes[1] + lanes[2] + lanes[3]
}

fn main() {
    let nthreads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let per = H.div_ceil(nthreads);
    let mut total = 0.0f64;
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(nthreads);
        for t in 0..nthreads {
            let y0 = (t * per).min(H);
            let y1 = (y0 + per).min(H);
            if y0 >= y1 {
                continue;
            }
            handles.push(scope.spawn(move || unsafe { trace(y0, y1) }));
        }
        for h in handles {
            total += h.join().unwrap();
        }
    });
    println!("{:.3}", total);
}
