/* Best-of n-body, 4 bodies, 10,000,000 steps.  Expected (native/FMA):
 *     11211.531383831 / 11211.531383844
 *
 * Same recurrence and the same per-body update order as the reference, but
 * all four bodies live in the four lanes of six AVX2 vectors
 * (X, Y, Z, VX, VY, VZ).  For source body j = 0..3 the code computes the
 * displacement and inverse-cube factor to all four bodies at once and applies
 *
 *     V[k] -= (X[k] - X[j]) * m[j] * mag[k]      (k != j)
 *
 * Since dx[k] == 0 exactly for k == j the self term is a harmless +-0; d2 is
 * forced to 1.0 on the diagonal only to keep mag finite.
 *
 * Why the order matches: body k's velocity receives the contributions from
 * sources 0,1,...,k-1 (added) and then k+1,...,3 (subtracted), exactly the
 * reference order (0,1),(0,2),(0,3),(1,2),(1,3),(2,3).  The only deviation is
 * FMA contraction, which is why the printed value is the expected FMA tail.
 * The SIMD form removes 8 of the 12 scalar square roots/divisions per step
 * and needs no stack spills for the 24 state doubles.
 */
#include <stdio.h>
#include <math.h>
#ifdef __AVX2__
#include <immintrin.h>
#endif

int main(void)
{
    double xs[4] = {0.0, 2.0, 4.0, 6.0};
    double ys[4] = {0.0, 1.0, -1.0, 0.5};
    double zs[4] = {0.0, 0.1, -0.1, 0.2};
    double vxs[4] = {0.0, 0.01, -0.01, 0.02};
    double vys[4] = {0.0, 0.005, -0.005, 0.001};
    double vzs[4] = {0.0, 0.001, -0.001, 0.0005};
    const double ms[4] = {100.0, 0.5, 0.3, 0.2};
    const double dt = 0.001;

#ifdef __AVX2__
    __m256d X  = _mm256_loadu_pd(xs);
    __m256d Y  = _mm256_loadu_pd(ys);
    __m256d Z  = _mm256_loadu_pd(zs);
    __m256d VX = _mm256_loadu_pd(vxs);
    __m256d VY = _mm256_loadu_pd(vys);
    __m256d VZ = _mm256_loadu_pd(vzs);

    const __m256d ONE = _mm256_set1_pd(1.0);
    const __m256d DT  = _mm256_set1_pd(dt);
    const __m256d M0  = _mm256_set1_pd(ms[0]);
    const __m256d M1  = _mm256_set1_pd(ms[1]);
    const __m256d M2  = _mm256_set1_pd(ms[2]);
    const __m256d M3  = _mm256_set1_pd(ms[3]);

/* All lanes of `v` are set to lane `j` of the permutation immediate. */
#define BCAST(v, j) _mm256_permute4x64_pd((v), (j) * 0x55)

/* One source body j: broadcast it, form the three displacement vectors,
 * compute d2 / d / mag 4-wide, then apply the three velocity updates. */
#define NB_STEP(j, MJ)                                                       \
    do {                                                                     \
        const __m256d bx = BCAST(X, (j));                                    \
        const __m256d by = BCAST(Y, (j));                                    \
        const __m256d bz = BCAST(Z, (j));                                    \
        const __m256d dx = _mm256_sub_pd(X, bx);                             \
        const __m256d dy = _mm256_sub_pd(Y, by);                             \
        const __m256d dz = _mm256_sub_pd(Z, bz);                             \
        __m256d d2 = _mm256_fmadd_pd(                                        \
            dz, dz, _mm256_fmadd_pd(dy, dy, _mm256_mul_pd(dx, dx)));         \
        d2 = _mm256_blend_pd(d2, ONE, 1 << (j));  /* self lane: keep finite */\
        const __m256d d = _mm256_sqrt_pd(d2);                                \
        const __m256d mag = _mm256_div_pd(DT, _mm256_mul_pd(d2, d));         \
        VX = _mm256_fnmadd_pd(_mm256_mul_pd(dx, (MJ)), mag, VX);             \
        VY = _mm256_fnmadd_pd(_mm256_mul_pd(dy, (MJ)), mag, VY);             \
        VZ = _mm256_fnmadd_pd(_mm256_mul_pd(dz, (MJ)), mag, VZ);             \
    } while (0)

    for (long long s = 0; s < 10000000LL; s++) {
        NB_STEP(0, M0);
        NB_STEP(1, M1);
        NB_STEP(2, M2);
        NB_STEP(3, M3);
        X = _mm256_fmadd_pd(DT, VX, X);
        Y = _mm256_fmadd_pd(DT, VY, Y);
        Z = _mm256_fmadd_pd(DT, VZ, Z);
    }

    _mm256_storeu_pd(xs, X);
    _mm256_storeu_pd(ys, Y);
    _mm256_storeu_pd(zs, Z);
    _mm256_storeu_pd(vxs, VX);
    _mm256_storeu_pd(vys, VY);
    _mm256_storeu_pd(vzs, VZ);
#else
    for (long long s = 0; s < 10000000LL; s++) {
        for (int i = 0; i < 4; i++)
            for (int j = i + 1; j < 4; j++) {
                const double dx = xs[i] - xs[j], dy = ys[i] - ys[j],
                             dz = zs[i] - zs[j];
                const double d2 = dx * dx + dy * dy + dz * dz;
                const double d = sqrt(d2);
                const double mag = dt / (d2 * d);
                vxs[i] -= dx * ms[j] * mag; vys[i] -= dy * ms[j] * mag;
                vzs[i] -= dz * ms[j] * mag;
                vxs[j] += dx * ms[i] * mag; vys[j] += dy * ms[i] * mag;
                vzs[j] += dz * ms[i] * mag;
            }
        for (int i = 0; i < 4; i++) {
            xs[i] += dt * vxs[i];
            ys[i] += dt * vys[i];
            zs[i] += dt * vzs[i];
        }
    }
#endif

    double e = 0.0;
    e += 0.5 * ms[0] * (vxs[0] * vxs[0] + vys[0] * vys[0] + vzs[0] * vzs[0]);
    e += 0.5 * ms[1] * (vxs[1] * vxs[1] + vys[1] * vys[1] + vzs[1] * vzs[1]);
    e += 0.5 * ms[2] * (vxs[2] * vxs[2] + vys[2] * vys[2] + vzs[2] * vzs[2]);
    e += 0.5 * ms[3] * (vxs[3] * vxs[3] + vys[3] * vys[3] + vzs[3] * vzs[3]);
    e -= ms[0] * ms[1] / sqrt((xs[0] - xs[1]) * (xs[0] - xs[1]) +
                              (ys[0] - ys[1]) * (ys[0] - ys[1]) +
                              (zs[0] - zs[1]) * (zs[0] - zs[1]));
    e -= ms[0] * ms[2] / sqrt((xs[0] - xs[2]) * (xs[0] - xs[2]) +
                              (ys[0] - ys[2]) * (ys[0] - ys[2]) +
                              (zs[0] - zs[2]) * (zs[0] - zs[2]));
    e -= ms[0] * ms[3] / sqrt((xs[0] - xs[3]) * (xs[0] - xs[3]) +
                              (ys[0] - ys[3]) * (ys[0] - ys[3]) +
                              (zs[0] - zs[3]) * (zs[0] - zs[3]));
    e -= ms[1] * ms[2] / sqrt((xs[1] - xs[2]) * (xs[1] - xs[2]) +
                              (ys[1] - ys[2]) * (ys[1] - ys[2]) +
                              (zs[1] - zs[2]) * (zs[1] - zs[2]));
    e -= ms[1] * ms[3] / sqrt((xs[1] - xs[3]) * (xs[1] - xs[3]) +
                              (ys[1] - ys[3]) * (ys[1] - ys[3]) +
                              (zs[1] - zs[3]) * (zs[1] - zs[3]));
    e -= ms[2] * ms[3] / sqrt((xs[2] - xs[3]) * (xs[2] - xs[3]) +
                              (ys[2] - ys[3]) * (ys[2] - ys[3]) +
                              (zs[2] - zs[3]) * (zs[2] - zs[3]));

    printf("%.9f\n", e);
    return 0;
}
