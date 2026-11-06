/* n-body, 4 bodies, 10,000,000 steps of symplectic Euler (kick then drift).
 *
 * The six pairwise interactions are independent within a step, so they are
 * evaluated as one 6-lane vector: lane k carries pair (i,j) of
 * {01,02,03,12,13,23}.  Index vectors permute the four positions into the
 * i-side and j-side of every pair, the squared distance, sqrt and dt/(d^2*d)
 * are computed for all six pairs at once, and the six velocity increments are
 * then accumulated into the four bodies in exactly the reference's i/j order,
 * so the trajectory stays bit-identical to the scalar non-FMA build.
 *
 * Expected: 11211.531383831 (non-FMA) or 11211.531383844 (FMA build)
 */
#include <stdio.h>
#pragma GCC optimize("fp-contract=off")
#include <math.h>
#include <immintrin.h>

#define STEPS 10000000
#define NB 4

int main(void) {
    double x[8] __attribute__((aligned(64)))  = {0.0, 2.0, 4.0, 6.0};
    double y[8] __attribute__((aligned(64)))  = {0.0, 1.0, -1.0, 0.5};
    double z[8] __attribute__((aligned(64)))  = {0.0, 0.1, -0.1, 0.2};
    double vx[8] __attribute__((aligned(64))) = {0.0, 0.01, -0.01, 0.02};
    double vy[8] __attribute__((aligned(64))) = {0.0, 0.005, -0.005, 0.001};
    double vz[8] __attribute__((aligned(64))) = {0.0, 0.001, -0.001, 0.0005};
    const double m[NB] = {100.0, 0.5, 0.3, 0.2};
    const double dt = 0.001;

    /* lane order: 01 02 03 12 13 23 (pairs 4,5 are padding) */
    const __m512i iv = _mm512_set_epi64(0, 0, 2, 1, 1, 0, 0, 0);
    const __m512i jv = _mm512_set_epi64(0, 0, 3, 3, 2, 3, 2, 1);
    const __m512d mj = _mm512_set_pd(0.0, 0.0, m[3], m[3], m[2], m[3], m[2], m[1]);
    const __m512d mi = _mm512_set_pd(0.0, 0.0, m[2], m[1], m[1], m[0], m[0], m[0]);
    const __m512d dtv = _mm512_set1_pd(dt);

    double pxv[8] __attribute__((aligned(64))), pyv[8] __attribute__((aligned(64)));
    double pzv[8] __attribute__((aligned(64))), qxv[8] __attribute__((aligned(64)));
    double qyv[8] __attribute__((aligned(64))), qzv[8] __attribute__((aligned(64)));

    for (int s = 0; s < STEPS; s++) {
        const __m512d xa = _mm512_load_pd(x);
        const __m512d ya = _mm512_load_pd(y);
        const __m512d za = _mm512_load_pd(z);

        const __m512d dx = _mm512_sub_pd(_mm512_permutexvar_pd(iv, xa),
                                         _mm512_permutexvar_pd(jv, xa));
        const __m512d dy = _mm512_sub_pd(_mm512_permutexvar_pd(iv, ya),
                                         _mm512_permutexvar_pd(jv, ya));
        const __m512d dz = _mm512_sub_pd(_mm512_permutexvar_pd(iv, za),
                                         _mm512_permutexvar_pd(jv, za));

        const __m512d d2 = _mm512_add_pd(_mm512_add_pd(_mm512_mul_pd(dx, dx),
                                                       _mm512_mul_pd(dy, dy)),
                                         _mm512_mul_pd(dz, dz));
        const __m512d d = _mm512_sqrt_pd(d2);
        const __m512d mag = _mm512_div_pd(dtv, _mm512_mul_pd(d2, d));

        const __m512d px = _mm512_mul_pd(_mm512_mul_pd(dx, mj), mag);
        const __m512d py = _mm512_mul_pd(_mm512_mul_pd(dy, mj), mag);
        const __m512d pz = _mm512_mul_pd(_mm512_mul_pd(dz, mj), mag);
        const __m512d qx = _mm512_mul_pd(_mm512_mul_pd(dx, mi), mag);
        const __m512d qy = _mm512_mul_pd(_mm512_mul_pd(dy, mi), mag);
        const __m512d qz = _mm512_mul_pd(_mm512_mul_pd(dz, mi), mag);

        _mm512_store_pd(pxv, px); _mm512_store_pd(pyv, py); _mm512_store_pd(pzv, pz);
        _mm512_store_pd(qxv, qx); _mm512_store_pd(qyv, qy); _mm512_store_pd(qzv, qz);

        /* velocity updates in the reference order */
        vx[0] -= pxv[0]; vy[0] -= pyv[0]; vz[0] -= pzv[0];
        vx[1] += qxv[0]; vy[1] += qyv[0]; vz[1] += qzv[0];

        vx[0] -= pxv[1]; vy[0] -= pyv[1]; vz[0] -= pzv[1];
        vx[2] += qxv[1]; vy[2] += qyv[1]; vz[2] += qzv[1];

        vx[0] -= pxv[2]; vy[0] -= pyv[2]; vz[0] -= pzv[2];
        vx[3] += qxv[2]; vy[3] += qyv[2]; vz[3] += qzv[2];

        vx[1] -= pxv[3]; vy[1] -= pyv[3]; vz[1] -= pzv[3];
        vx[2] += qxv[3]; vy[2] += qyv[3]; vz[2] += qzv[3];

        vx[1] -= pxv[4]; vy[1] -= pyv[4]; vz[1] -= pzv[4];
        vx[3] += qxv[4]; vy[3] += qyv[4]; vz[3] += qzv[4];

        vx[2] -= pxv[5]; vy[2] -= pyv[5]; vz[2] -= pzv[5];
        vx[3] += qxv[5]; vy[3] += qyv[5]; vz[3] += qzv[5];

        for (int i = 0; i < NB; i++) {
            x[i] += dt * vx[i];
            y[i] += dt * vy[i];
            z[i] += dt * vz[i];
        }
    }

    double e = 0.0;
    for (int i = 0; i < NB; i++)
        e += 0.5 * m[i] * (vx[i] * vx[i] + vy[i] * vy[i] + vz[i] * vz[i]);
    for (int i = 0; i < NB; i++)
        for (int j = i + 1; j < NB; j++) {
            double dx = x[i] - x[j];
            double dy = y[i] - y[j];
            double dz = z[i] - z[j];
            e -= m[i] * m[j] / sqrt(dx * dx + dy * dy + dz * dz);
        }
    printf("%.9f\n", e);
    return 0;
}
