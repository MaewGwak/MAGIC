/* Ray tracer 1536x1536, 5 spheres.
 *
 * OpenMP over rows with a 4-pixel AVX2 kernel that mirrors the reference
 * arithmetic operation-for-operation (no FMA contraction, same parens, same
 * sqrt/div placement), so the hit selection is bit-identical to the scalar
 * reference and only the final reduction order changes.
 *
 * The shadow ray is the only thing that needs the hit sphere: instead of
 * branching, the 5 candidate centers are selected with 4 blends each and the
 * shading is evaluated once.  Lanes that miss are masked to 0.
 *
 * Expected: 336141.941
 */
#include <stdio.h>
#include <stdlib.h>
#include <immintrin.h>
#include <omp.h>

#define W 1536
#define H 1536
#define S 5

static const double Cx[S] = {0.0, 1.5, -1.2, 2.5, -2.0};
static const double Cy[S] = {0.0, 0.3, -0.6, -0.8, 1.2};
static const double Cz[S] = {3.0, 3.2, 2.8, 4.0, 3.5};
static const double Rr[S] = {1.0, 0.6, 0.7, 0.5, 0.4};

static inline double hsum4(__m256d v) {
    __m128d lo = _mm256_castpd256_pd128(v);
    __m128d hi = _mm256_extractf128_pd(v, 1);
    __m128d s = _mm_add_pd(lo, hi);
    return _mm_cvtsd_f64(_mm_add_sd(s, _mm_unpackhi_pd(s, s)));
}


/* The host has 8 physical cores / 16 SMT threads.  Measured on this machine,
   16 OpenMP threads is consistently slower than 12 (SMT siblings share the
   L1/L2 and the host is shared), so cap at 1.5x physical cores unless the
   caller sets OMP_NUM_THREADS explicitly. */
static void limit_threads(void) {
    if (getenv("OMP_NUM_THREADS")) return;
    int cores = 0;
    FILE *f = fopen("/proc/cpuinfo", "r");
    if (f) {
        char line[256];
        while (fgets(line, sizeof line, f))
            if (sscanf(line, "cpu cores : %d", &cores) == 1) break;
        fclose(f);
    }
    int cap = cores > 0 ? cores + cores / 2 : 0;
    if (cap > 0 && omp_get_max_threads() > cap) omp_set_num_threads(cap);
}

int main(void) {
    limit_threads();
    double ox[S], oy[S], oz[S], cval[S];
    for (int s = 0; s < S; s++) {
        ox[s] = -Cx[s]; oy[s] = -Cy[s]; oz[s] = -Cz[s];
        /* same parens as the reference: ((ox^2 + oy^2) + oz^2) - rr^2 */
        cval[s] = ox[s] * ox[s] + oy[s] * oy[s] + oz[s] * oz[s] - Rr[s] * Rr[s];
    }

    const __m256d v768 = _mm256_set1_pd(768.0);
    const __m256d vhalf = _mm256_set1_pd(0.5 * W);
    const __m256d v15 = _mm256_set1_pd(1.5);
    const __m256d zero = _mm256_setzero_pd();
    const __m256d big = _mm256_set1_pd(1000000000.0);
    const __m256d lx = _mm256_set1_pd(5.0), ly = _mm256_set1_pd(5.0), lz = _mm256_set1_pd(0.0);

    double total = 0.0;
    #pragma omp parallel for reduction(+:total) schedule(static)
    for (int y = 0; y < H; y++) {
        const __m256d vv = _mm256_div_pd(_mm256_sub_pd(_mm256_set1_pd((double)y), v768), v768);
        __m256d acc = zero;
        for (int x = 0; x < W; x += 4) {
            const __m256d xv = _mm256_set_pd((double)(x + 3), (double)(x + 2),
                                             (double)(x + 1), (double)x);
            const __m256d u = _mm256_div_pd(_mm256_sub_pd(xv, vhalf), vhalf);
            __m256d dx = u, dy = vv, dz = v15;
            __m256d len = _mm256_sqrt_pd(_mm256_add_pd(_mm256_add_pd(_mm256_mul_pd(dx, dx),
                                                                    _mm256_mul_pd(dy, dy)),
                                                       _mm256_mul_pd(dz, dz)));
            dx = _mm256_div_pd(dx, len);
            dy = _mm256_div_pd(dy, len);
            dz = _mm256_div_pd(dz, len);

            __m256d best = big;
            __m256i hit = _mm256_set1_epi64x(-1);
            for (int s = 0; s < S; s++) {
                const __m256d b = _mm256_add_pd(_mm256_add_pd(_mm256_mul_pd(_mm256_set1_pd(ox[s]), dx),
                                                              _mm256_mul_pd(_mm256_set1_pd(oy[s]), dy)),
                                                _mm256_mul_pd(_mm256_set1_pd(oz[s]), dz));
                const __m256d disc = _mm256_sub_pd(_mm256_mul_pd(b, b), _mm256_set1_pd(cval[s]));
                const __m256d sq = _mm256_sqrt_pd(disc);
                const __m256d t = _mm256_sub_pd(_mm256_sub_pd(zero, b), sq);
                const __m256d m = _mm256_and_pd(_mm256_cmp_pd(disc, zero, _CMP_GT_OQ),
                                  _mm256_and_pd(_mm256_cmp_pd(t, zero, _CMP_GT_OQ),
                                                _mm256_cmp_pd(t, best, _CMP_LT_OQ)));
                best = _mm256_blendv_pd(best, t, m);
                hit = _mm256_castpd_si256(
                    _mm256_blendv_pd(_mm256_castsi256_pd(hit),
                                     _mm256_castsi256_pd(_mm256_set1_epi64x(s)), m));
            }

            /* select the hit sphere's center with blends, then shade once */
            __m256d hx = _mm256_set1_pd(Cx[0]);
            __m256d hy = _mm256_set1_pd(Cy[0]);
            __m256d hz = _mm256_set1_pd(Cz[0]);
            for (int s = 1; s < S; s++) {
                const __m256d m = _mm256_castsi256_pd(_mm256_cmpeq_epi64(hit, _mm256_set1_epi64x(s)));
                hx = _mm256_blendv_pd(hx, _mm256_set1_pd(Cx[s]), m);
                hy = _mm256_blendv_pd(hy, _mm256_set1_pd(Cy[s]), m);
                hz = _mm256_blendv_pd(hz, _mm256_set1_pd(Cz[s]), m);
            }
            const __m256d anymask =
                _mm256_castsi256_pd(_mm256_cmpgt_epi64(hit, _mm256_set1_epi64x(-1)));

            const __m256d px = _mm256_mul_pd(dx, best);
            const __m256d py = _mm256_mul_pd(dy, best);
            const __m256d pz = _mm256_mul_pd(dz, best);
            __m256d nx = _mm256_sub_pd(px, hx);
            __m256d ny = _mm256_sub_pd(py, hy);
            __m256d nz = _mm256_sub_pd(pz, hz);
            const __m256d nl = _mm256_sqrt_pd(_mm256_add_pd(_mm256_add_pd(_mm256_mul_pd(nx, nx),
                                                                          _mm256_mul_pd(ny, ny)),
                                                             _mm256_mul_pd(nz, nz)));
            nx = _mm256_div_pd(nx, nl);
            ny = _mm256_div_pd(ny, nl);
            nz = _mm256_div_pd(nz, nl);
            const __m256d ldx = _mm256_sub_pd(lx, px);
            const __m256d ldy = _mm256_sub_pd(ly, py);
            const __m256d ldz = _mm256_sub_pd(lz, pz);
            const __m256d ll = _mm256_sqrt_pd(_mm256_add_pd(_mm256_add_pd(_mm256_mul_pd(ldx, ldx),
                                                                         _mm256_mul_pd(ldy, ldy)),
                                                            _mm256_mul_pd(ldz, ldz)));
            __m256d d = _mm256_div_pd(_mm256_add_pd(_mm256_add_pd(_mm256_mul_pd(nx, ldx),
                                                                  _mm256_mul_pd(ny, ldy)),
                                                     _mm256_mul_pd(nz, ldz)), ll);
            d = _mm256_max_pd(d, zero);
            acc = _mm256_add_pd(acc, _mm256_and_pd(d, anymask));
        }
        total += hsum4(acc);
    }

    printf("%.3f\n", total);
    return 0;
}
