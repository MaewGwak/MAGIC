/* Best-of ray tracer, 1536 x 1536, 5 spheres.  Expected 336141.941.
 *
 * The reference formula is kept exactly (normalised ray, quadratic sphere
 * test, nearest hit, Lambert term), but:
 *   - per row, the exact u interval in which a sphere can be hit is derived
 *     from the sphere's silhouette cone (a quadratic in u); spheres whose
 *     interval does not touch a 4-pixel group are skipped entirely;
 *   - pixels are processed 4 at a time with AVX2 (sets of 4 are branch-free);
 *   - rows are distributed with OpenMP.
 * Both the culling and the SIMD are exact up to last-bit rounding; the
 * printed value is bit-comparable with the reference to far better than the
 * allowed 1e-9 relative tolerance.
 */
#include <stdio.h>
#include <math.h>
#include <omp.h>
#ifdef __AVX2__
#include <immintrin.h>
#endif

#define W 1536
#define H 1536
#define S 5
#define HW 768.0

static const double CX[S] = {0.0, 1.5, -1.2, 2.5, -2.0};
static const double CY[S] = {0.0, 0.3, -0.6, -0.8, 1.2};
static const double CZ[S] = {3.0, 3.2, 2.8, 4.0, 3.5};
static const double RR[S] = {1.0, 0.6, 0.7, 0.5, 0.4};
static const double LX = 5.0, LY = 5.0, LZ = 0.0;

#ifdef __AVX2__

static inline double hsum256(__m256d v)
{
    __m128d lo = _mm256_castpd256_pd128(v);
    __m128d hi = _mm256_extractf128_pd(v, 1);
    __m128d s = _mm_add_pd(lo, hi);
    s = _mm_hadd_pd(s, s);
    return _mm_cvtsd_f64(s);
}

static void render_avx2(double *out)
{
    double total = 0.0;

#pragma omp parallel for schedule(static) num_threads(8) reduction(+:total)
    for (int y = 0; y < H; y++) {
        const double v = ((double)y - HW) / HW;

        /* Exact u interval (in pixels) in which each sphere can be hit. */
        int sx0[S], sx1[S], active[S];
        int nact = 0;
        for (int s = 0; s < S; s++) {
            const double c2 = CX[s] * CX[s] + CY[s] * CY[s] + CZ[s] * CZ[s]
                            - RR[s] * RR[s];
            const double s0 = CY[s] * v + CZ[s] * 1.5;
            const double A  = CX[s] * CX[s] - c2;
            const double B  = 2.0 * CX[s] * s0;
            const double E  = s0 * s0 - c2 * (v * v + 2.25);
            if (A >= 0.0) continue;
            const double disc = B * B - 4.0 * A * E;
            if (disc <= 0.0) continue;
            const double sq = sqrt(disc);
            double u1 = (-B + sq) / (2.0 * A);
            double u2 = (-B - sq) / (2.0 * A);
            if (u1 > u2) { const double t = u1; u1 = u2; u2 = t; }
            int lo = (int)ceil(HW + HW * u1) - 2;
            int hi = (int)floor(HW + HW * u2) + 2;
            if (lo < 0) lo = 0;
            if (hi > W - 1) hi = W - 1;
            if (lo > hi) continue;
            sx0[s] = lo; sx1[s] = hi; active[nact++] = s;
        }

        __m256d acc = _mm256_setzero_pd();

        for (int x = 0; x < W; x += 4) {
            int list[S], na = 0;
            for (int k = 0; k < nact; k++) {
                const int s = active[k];
                if (sx1[s] >= x && sx0[s] <= x + 3) list[na++] = s;
            }
            if (na == 0) continue;

            const __m256d xv = _mm256_set_pd((double)(x + 3), (double)(x + 2),
                                             (double)(x + 1), (double)x);
            __m256d dx = _mm256_div_pd(_mm256_sub_pd(xv, _mm256_set1_pd(HW)),
                                       _mm256_set1_pd(HW));
            __m256d dy = _mm256_set1_pd(v);
            __m256d dz = _mm256_set1_pd(1.5);
            __m256d len2 = _mm256_add_pd(
                _mm256_add_pd(_mm256_mul_pd(dx, dx), _mm256_mul_pd(dy, dy)),
                _mm256_mul_pd(dz, dz));
            __m256d len = _mm256_sqrt_pd(len2);
            dx = _mm256_div_pd(dx, len);
            dy = _mm256_div_pd(dy, len);
            dz = _mm256_div_pd(dz, len);

            __m256d best  = _mm256_set1_pd(1e9);
            __m256d valid = _mm256_setzero_pd();
            __m256d hitv  = _mm256_setzero_pd();
            const __m256d zero = _mm256_setzero_pd();

            for (int k = 0; k < na; k++) {
                const int s = list[k];
                const __m256d ox = _mm256_set1_pd(-CX[s]);
                const __m256d oy = _mm256_set1_pd(-CY[s]);
                const __m256d oz = _mm256_set1_pd(-CZ[s]);
                const double  c  = CX[s] * CX[s] + CY[s] * CY[s] + CZ[s] * CZ[s]
                                 - RR[s] * RR[s];

                __m256d b = _mm256_add_pd(
                    _mm256_add_pd(_mm256_mul_pd(ox, dx), _mm256_mul_pd(oy, dy)),
                    _mm256_mul_pd(oz, dz));
                __m256d disc = _mm256_sub_pd(_mm256_mul_pd(b, b),
                                             _mm256_set1_pd(c));
                __m256d t = _mm256_sub_pd(_mm256_sub_pd(zero, b),
                                          _mm256_sqrt_pd(disc));
                __m256d m = _mm256_and_pd(
                    _mm256_cmp_pd(disc, zero, _CMP_GT_OQ),
                    _mm256_and_pd(_mm256_cmp_pd(t, zero, _CMP_GT_OQ),
                                  _mm256_cmp_pd(t, best, _CMP_LT_OQ)));
                best  = _mm256_blendv_pd(best, t, m);
                valid = _mm256_or_pd(valid, m);
                hitv  = _mm256_blendv_pd(hitv, _mm256_set1_pd((double)s), m);
            }

            /* Shading.  Miss lanes get best = 0 and hit = 0 (safe gather). */
            best = _mm256_and_pd(best, valid);
            const __m128i hit = _mm256_cvtpd_epi32(hitv);
            __m256d px = _mm256_mul_pd(dx, best);
            __m256d py = _mm256_mul_pd(dy, best);
            __m256d pz = _mm256_mul_pd(dz, best);

            __m256d cxh = _mm256_i32gather_pd(CX, hit, 8);
            __m256d cyh = _mm256_i32gather_pd(CY, hit, 8);
            __m256d czh = _mm256_i32gather_pd(CZ, hit, 8);
            __m256d nx = _mm256_sub_pd(px, cxh);
            __m256d ny = _mm256_sub_pd(py, cyh);
            __m256d nz = _mm256_sub_pd(pz, czh);
            __m256d nl2 = _mm256_add_pd(
                _mm256_add_pd(_mm256_mul_pd(nx, nx), _mm256_mul_pd(ny, ny)),
                _mm256_mul_pd(nz, nz));
            __m256d inv = _mm256_div_pd(_mm256_set1_pd(1.0),
                                        _mm256_sqrt_pd(nl2));
            nx = _mm256_mul_pd(nx, inv);
            ny = _mm256_mul_pd(ny, inv);
            nz = _mm256_mul_pd(nz, inv);

            __m256d ldx = _mm256_sub_pd(_mm256_set1_pd(LX), px);
            __m256d ldy = _mm256_sub_pd(_mm256_set1_pd(LY), py);
            __m256d ldz = _mm256_sub_pd(_mm256_set1_pd(LZ), pz);
            __m256d ll = _mm256_sqrt_pd(_mm256_add_pd(
                _mm256_add_pd(_mm256_mul_pd(ldx, ldx),
                              _mm256_mul_pd(ldy, ldy)),
                _mm256_mul_pd(ldz, ldz)));
            __m256d d = _mm256_div_pd(
                _mm256_add_pd(_mm256_add_pd(_mm256_mul_pd(nx, ldx),
                                            _mm256_mul_pd(ny, ldy)),
                              _mm256_mul_pd(nz, ldz)),
                ll);
            d = _mm256_max_pd(d, zero);
            d = _mm256_and_pd(d, valid);
            acc = _mm256_add_pd(acc, d);
        }

        total += hsum256(acc);
    }

    *out += total;
}

#else  /* portable fallback */

static void render_scalar(double *out)
{
    double total = 0.0;

#pragma omp parallel for schedule(static) num_threads(8) reduction(+:total)
    for (int y = 0; y < H; y++) {
        for (int x = 0; x < W; x++) {
            const double u = ((double)x - HW) / HW;
            const double v = ((double)y - HW) / HW;
            double dx = u, dy = v, dz = 1.5;
            const double len = sqrt(dx * dx + dy * dy + dz * dz);
            dx /= len; dy /= len; dz /= len;

            double best = 1e9;
            int hit = -1;
            for (int s = 0; s < S; s++) {
                const double ox = -CX[s], oy = -CY[s], oz = -CZ[s];
                const double b = ox * dx + oy * dy + oz * dz;
                const double c = ox * ox + oy * oy + oz * oz - RR[s] * RR[s];
                const double disc = b * b - c;
                if (disc > 0.0) {
                    const double t = -b - sqrt(disc);
                    if (t > 0.0 && t < best) { best = t; hit = s; }
                }
            }
            double col = 0.0;
            if (hit >= 0) {
                const double px = dx * best, py = dy * best, pz = dz * best;
                double nx = px - CX[hit], ny = py - CY[hit], nz = pz - CZ[hit];
                const double nl = sqrt(nx * nx + ny * ny + nz * nz);
                nx /= nl; ny /= nl; nz /= nl;
                const double ldx = LX - px, ldy = LY - py, ldz = LZ - pz;
                const double ll = sqrt(ldx * ldx + ldy * ldy + ldz * ldz);
                double d = (nx * ldx + ny * ldy + nz * ldz) / ll;
                if (d < 0.0) d = 0.0;
                col = d;
            }
            total += col;
        }
    }
    *out += total;
}

#endif

int main(void)
{
    double total = 0.0;
#ifdef __AVX2__
    render_avx2(&total);
#else
    render_scalar(&total);
#endif
    printf("%.3f\n", total);
    return 0;
}
