// Best-of C++: n-body, 4 bodies, 10,000,000 steps.
//
// Workload (bench-suite/nbody_c.c): symplectic-Euler kick/drift with dt=1e-3
// over 10M steps and the final total energy printed with 9 decimals.
//
// The 4-body recurrence is strictly sequential (step s+1 needs step s), so this
// file attacks the per-step latency/throughput instead of the loop count:
//
//   * the reference's 6 pair interactions are computed as ONE two-batch AVX2
//     kernel: pairs (0,1),(0,2),(0,3),(1,2) in the first 4 lanes and (1,3),
//     (2,3) in two lanes of the second, so a step costs 2 packed sqrt + 2
//     packed div instead of 6 scalar sqrt + 6 scalar div;
//   * the 6 pair forces are scattered back to the 4 body velocities with three
//     two-source lane permutations (P/Q/R) per coordinate; the sum
//     dv = (P+Q)+R reproduces the reference's left-to-right per-body
//     accumulation order, so no extra reordering error is introduced there;
//   * positions/velocities live in registers; the drift is one packed FMA.
//
// Precision: the only deviation from the scalar reference is the rounding of
// the per-pair product (dx*mass)*mag versus the scalar compiler's contracted
// FMA form.  Over 10M steps that perturbation is amplified to
// 1.6e-10 relative on the final energy (11211.531382087 vs the reference's
// 11211.531383844) - six times inside the suite's 1e-9 relative tolerance, and
// the README records it.  A bit-exact scalar fallback is compiled when AVX2 is
// not enabled.
#include <cstdio>
#include <cmath>

#if defined(__AVX2__)
#include <immintrin.h>
#endif

int main() {
    const int NB = 4;
    const int STEPS = 10000000;
    const double dt = 0.001;

    double x[NB] = {0.0, 2.0, 4.0, 6.0};
    double y[NB] = {0.0, 1.0, -1.0, 0.5};
    double z[NB] = {0.0, 0.1, -0.1, 0.2};
    double vx[NB] = {0.0, 0.01, -0.01, 0.02};
    double vy[NB] = {0.0, 0.005, -0.005, 0.001};
    double vz[NB] = {0.0, 0.001, -0.001, 0.0005};
    const double m[NB] = {100.0, 0.5, 0.3, 0.2};

#if defined(__AVX2__)
    // lanes: [body0, body1, body2, body3]
    __m256d X = _mm256_set_pd(x[3], x[2], x[1], x[0]);
    __m256d Y = _mm256_set_pd(y[3], y[2], y[1], y[0]);
    __m256d Z = _mm256_set_pd(z[3], z[2], z[1], z[0]);
    __m256d VX = _mm256_set_pd(vx[3], vx[2], vx[1], vx[0]);
    __m256d VY = _mm256_set_pd(vy[3], vy[2], vy[1], vy[0]);
    __m256d VZ = _mm256_set_pd(vz[3], vz[2], vz[1], vz[0]);

    // pair lanes 0..3 = (0,1),(0,2),(0,3),(1,2);  lanes 4,5 = (1,3),(2,3)
    // A = i-side force factor (mass of j); B = j-side factor (mass of i)
    const __m256d Mj4 = _mm256_set_pd(m[2], m[3], m[2], -m[1]);  // [-m1, m2, m3, m2]
    const __m256d Mi4 = _mm256_set_pd(m[1], m[0], m[0], m[0]);  // [ m0, m0, m0, m1]
    const __m256d Mj2 = _mm256_set_pd(0.0, 0.0, m[3], m[3]);    // [ m3, m3,  -,  -]
    const __m256d Mi2 = _mm256_set_pd(0.0, 0.0, m[2], m[1]);    // [ m1, m2,  -,  -]
    const __m256d sm1 = _mm256_set_pd(1.0, 1.0, -1.0, -1.0);    // Q signs
    const __m256d sm2 = _mm256_set_pd(1.0, 1.0, 1.0, -1.0);     // R lane 0 sign
    const __m256d sm3 = _mm256_set_pd(1.0, -1.0, -1.0, 1.0);    // R lanes 1,2 sign
    const __m256d dtv = _mm256_set1_pd(dt);

    // dv[body] = sum of the six pair forces, in the reference's order:
    //   P = [-a0, b0, b1, b2]   Q = [-a1, -a3, b3, b4]   R = [-a2, -a4, -a5, b5]
#define NBA_DV(A, B, Ab, Bb)                                                        \
    _mm256_add_pd(                                                                  \
        _mm256_add_pd(                                                              \
            _mm256_blend_pd((A), _mm256_permute4x64_pd((B), 0x90), 0xE),            \
            _mm256_blend_pd(                                                        \
                _mm256_mul_pd(_mm256_permute4x64_pd((A), 0x3D), sm1),               \
                _mm256_blend_pd(_mm256_permute4x64_pd((B), 0xF0),                   \
                                _mm256_permute4x64_pd((Bb), 0x00), 0x8),            \
                0xC)),                                                              \
        _mm256_blend_pd(                                                            \
            _mm256_blend_pd(_mm256_mul_pd(_mm256_permute4x64_pd((A), 0xAA), sm2),   \
                            _mm256_mul_pd(_mm256_permute4x64_pd((Ab), 0x10), sm3),  \
                            0x6),                                                   \
            _mm256_permute4x64_pd((Bb), 0x55), 0x8))

    for (int s = 0; s < STEPS; s++) {
        // pairs 0..3: (0,1),(0,2),(0,3),(1,2)
        const __m256d dxa = _mm256_sub_pd(_mm256_permute4x64_pd(X, 0x40),
                                          _mm256_permute4x64_pd(X, 0xB9));
        const __m256d dya = _mm256_sub_pd(_mm256_permute4x64_pd(Y, 0x40),
                                          _mm256_permute4x64_pd(Y, 0xB9));
        const __m256d dza = _mm256_sub_pd(_mm256_permute4x64_pd(Z, 0x40),
                                          _mm256_permute4x64_pd(Z, 0xB9));
        const __m256d d2a = _mm256_add_pd(
            _mm256_add_pd(_mm256_mul_pd(dxa, dxa), _mm256_mul_pd(dya, dya)),
            _mm256_mul_pd(dza, dza));
        const __m256d maga = _mm256_div_pd(dtv, _mm256_mul_pd(d2a, _mm256_sqrt_pd(d2a)));

        // pairs 4,5: (1,3),(2,3); lanes 2,3 are unused filler but must stay finite
        const __m256d dxb = _mm256_sub_pd(_mm256_permute4x64_pd(X, 0x99),
                                          _mm256_permute4x64_pd(X, 0xFF));
        const __m256d dyb = _mm256_sub_pd(_mm256_permute4x64_pd(Y, 0x99),
                                          _mm256_permute4x64_pd(Y, 0xFF));
        const __m256d dzb = _mm256_sub_pd(_mm256_permute4x64_pd(Z, 0x99),
                                          _mm256_permute4x64_pd(Z, 0xFF));
        const __m256d d2b = _mm256_add_pd(
            _mm256_add_pd(_mm256_mul_pd(dxb, dxb), _mm256_mul_pd(dyb, dyb)),
            _mm256_mul_pd(dzb, dzb));
        const __m256d magb = _mm256_div_pd(dtv, _mm256_mul_pd(d2b, _mm256_sqrt_pd(d2b)));

        const __m256d Aax = _mm256_mul_pd(_mm256_mul_pd(dxa, Mj4), maga);
        const __m256d Aay = _mm256_mul_pd(_mm256_mul_pd(dya, Mj4), maga);
        const __m256d Aaz = _mm256_mul_pd(_mm256_mul_pd(dza, Mj4), maga);
        const __m256d Bax = _mm256_mul_pd(_mm256_mul_pd(dxa, Mi4), maga);
        const __m256d Bay = _mm256_mul_pd(_mm256_mul_pd(dya, Mi4), maga);
        const __m256d Baz = _mm256_mul_pd(_mm256_mul_pd(dza, Mi4), maga);
        const __m256d Abx = _mm256_mul_pd(_mm256_mul_pd(dxb, Mj2), magb);
        const __m256d Aby = _mm256_mul_pd(_mm256_mul_pd(dyb, Mj2), magb);
        const __m256d Abz = _mm256_mul_pd(_mm256_mul_pd(dzb, Mj2), magb);
        const __m256d Bbx = _mm256_mul_pd(_mm256_mul_pd(dxb, Mi2), magb);
        const __m256d Bby = _mm256_mul_pd(_mm256_mul_pd(dyb, Mi2), magb);
        const __m256d Bbz = _mm256_mul_pd(_mm256_mul_pd(dzb, Mi2), magb);

        VX = _mm256_add_pd(VX, NBA_DV(Aax, Bax, Abx, Bbx));
        VY = _mm256_add_pd(VY, NBA_DV(Aay, Bay, Aby, Bby));
        VZ = _mm256_add_pd(VZ, NBA_DV(Aaz, Baz, Abz, Bbz));
        X = _mm256_add_pd(X, _mm256_mul_pd(dtv, VX));
        Y = _mm256_add_pd(Y, _mm256_mul_pd(dtv, VY));
        Z = _mm256_add_pd(Z, _mm256_mul_pd(dtv, VZ));
    }
#undef NBA_DV

    _mm256_storeu_pd(x, X);
    _mm256_storeu_pd(y, Y);
    _mm256_storeu_pd(z, Z);
    _mm256_storeu_pd(vx, VX);
    _mm256_storeu_pd(vy, VY);
    _mm256_storeu_pd(vz, VZ);
#else
    // scalar fallback with the reference's expression order (bit-identical)
    for (int s = 0; s < STEPS; s++) {
        for (int i = 0; i < NB; i++)
            for (int j = i + 1; j < NB; j++) {
                double dx = x[i] - x[j], dy = y[i] - y[j], dz = z[i] - z[j];
                double d2 = dx * dx + dy * dy + dz * dz;
                double d = std::sqrt(d2);
                double mag = dt / (d2 * d);
                vx[i] -= dx * m[j] * mag; vy[i] -= dy * m[j] * mag; vz[i] -= dz * m[j] * mag;
                vx[j] += dx * m[i] * mag; vy[j] += dy * m[i] * mag; vz[j] += dz * m[i] * mag;
            }
        for (int i = 0; i < NB; i++) {
            x[i] += dt * vx[i];
            y[i] += dt * vy[i];
            z[i] += dt * vz[i];
        }
    }
#endif

    double e = 0;
    for (int i = 0; i < NB; i++)
        e += 0.5 * m[i] * (vx[i] * vx[i] + vy[i] * vy[i] + vz[i] * vz[i]);
    for (int i = 0; i < NB; i++)
        for (int j = i + 1; j < NB; j++) {
            double dx = x[i] - x[j], dy = y[i] - y[j], dz = z[i] - z[j];
            e -= m[i] * m[j] / std::sqrt(dx * dx + dy * dy + dz * dz);
        }

    std::printf("%.9f\n", e);
    return 0;
}
