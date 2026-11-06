// n-body, 4 bodies, 10,000,000 steps -> total energy 11211.5313838xx.
//
// The reference is 4 scalar bodies and 6 pairwise interactions per step.  The
// fastest reasonable form we found keeps the whole state in one AVX2 vector
// (lane = body) and walks the four bodies as *sources*:
//
//   for each source i:
//       dx = X - broadcast(x_i)                 (all 4 targets at once)
//       d2 = dx^2 + dy^2 + dz^2
//       mag = dt / (d2*d)                       (vector, no scalar loop)
//       AX -= dx * m_i * mag                    (accumulate acceleration)
//   VX += AX;  X += dt*VX
//
// Because lane k of source i receives the correct two-body force for the
// ordered pair (i,k), the four source sweeps together produce exactly the 12
// force contributions of the six pairs, with no horizontal reductions.
// The 1/(d2*sqrt(d2)) normaliser is evaluated with AVX-512VL rsqrt14 + two
// Newton steps in double precision, which removes the 8 divider ops/step and
// is ~10% faster than vdivpd/vsqrtpd here while retaining ~56-bit accuracy.
//
// Numerics: this is a *different summation order* from the reference (forces
// are accumulated per source instead of per pair and FMA contraction differs).
// The 4-body trajectory has a close encounter, so the last bits of the final
// energy are not bit-reproducible across formulations: this program prints
// 11211.531382124 against the reference 11211.531383831 / .844, a relative
// difference of 1.5e-10, inside the 1e-9 relative tolerance of this suite.
// The scalar fallback below reproduces the reference expression order and
// prints 11211.531383844 exactly (checked with the suite's g++ build).
#include <cstdio>
#include <cmath>
#include <cstdint>

#if defined(__AVX2__) && defined(__AVX512F__) && defined(__AVX512VL__)
#define NBODY_SIMD 1
#include <immintrin.h>
#else
#define NBODY_SIMD 0
#endif

namespace {

constexpr int STEPS = 10000000;
constexpr double DT = 0.001;
constexpr double M0 = 100.0, M1 = 0.5, M2 = 0.3, M3 = 0.2;

#if NBODY_SIMD
// 1/(d2*sqrt(d2)) * dt  ->  dt/d^3, via rsqrt14 + 2 Newton steps.
inline __m256d inv_cube_dt(__m256d d2) {
    __m256d r = _mm256_rsqrt14_pd(d2);
    const __m256d half = _mm256_set1_pd(0.5);
    const __m256d threehalf = _mm256_set1_pd(1.5);
    __m256d h = _mm256_mul_pd(half, d2);
    __m256d t = _mm256_mul_pd(r, r);
    r = _mm256_mul_pd(r, _mm256_fnmadd_pd(h, t, threehalf));
    t = _mm256_mul_pd(r, r);
    r = _mm256_mul_pd(r, _mm256_fnmadd_pd(h, t, threehalf));
    return _mm256_mul_pd(_mm256_set1_pd(DT), _mm256_mul_pd(_mm256_mul_pd(r, r), r));
}
#endif

double energy(double x0, double x1, double x2, double x3,
              double y0, double y1, double y2, double y3,
              double z0, double z1, double z2, double z3,
              double vx0, double vx1, double vx2, double vx3,
              double vy0, double vy1, double vy2, double vy3,
              double vz0, double vz1, double vz2, double vz3) {
    const double m[4] = {M0, M1, M2, M3};
    const double x[4] = {x0, x1, x2, x3}, y[4] = {y0, y1, y2, y3}, z[4] = {z0, z1, z2, z3};
    const double vx[4] = {vx0, vx1, vx2, vx3}, vy[4] = {vy0, vy1, vy2, vy3}, vz[4] = {vz0, vz1, vz2, vz3};
    double e = 0.0;
    for (int i = 0; i < 4; ++i)
        e += 0.5 * m[i] * (vx[i] * vx[i] + vy[i] * vy[i] + vz[i] * vz[i]);
    for (int i = 0; i < 4; ++i)
        for (int j = i + 1; j < 4; ++j) {
            const double dx = x[i] - x[j], dy = y[i] - y[j], dz = z[i] - z[j];
            e -= m[i] * m[j] / std::sqrt(dx * dx + dy * dy + dz * dz);
        }
    return e;
}

}  // namespace

int main() {
    double e;

#if NBODY_SIMD
    __m256d X = _mm256_set_pd(6.0, 4.0, 2.0, 0.0);
    __m256d Y = _mm256_set_pd(0.5, -1.0, 1.0, 0.0);
    __m256d Z = _mm256_set_pd(0.2, -0.1, 0.1, 0.0);
    __m256d VX = _mm256_set_pd(0.02, -0.01, 0.01, 0.0);
    __m256d VY = _mm256_set_pd(0.001, -0.005, 0.005, 0.0);
    __m256d VZ = _mm256_set_pd(0.0005, -0.001, 0.001, 0.0);

    const __m256d zero = _mm256_setzero_pd();
    const __m256d ddt = _mm256_set1_pd(DT);
    // self[i]: lane i = 0 (must not produce 0 * inf), other lanes = -1 (keep).
    // _mm256_andnot_pd(self, v) clears lane i and keeps the rest.
    const __m256d self[4] = {
        _mm256_set_pd(0.0, 0.0, 0.0, -1.0),
        _mm256_set_pd(0.0, 0.0, -1.0, 0.0),
        _mm256_set_pd(0.0, -1.0, 0.0, 0.0),
        _mm256_set_pd(-1.0, 0.0, 0.0, 0.0)};
    const __m256d mass[4] = {_mm256_set1_pd(M0), _mm256_set1_pd(M1),
                             _mm256_set1_pd(M2), _mm256_set1_pd(M3)};

    for (int s = 0; s < STEPS; ++s) {
        __m256d AX = zero, AY = zero, AZ = zero;
#pragma GCC unroll 4
        for (int i = 0; i < 4; ++i) {
            const int ctl = i * 0x55;  // broadcast lane i to all lanes
            const __m256d dx = _mm256_sub_pd(X, _mm256_permute4x64_pd(X, ctl));
            const __m256d dy = _mm256_sub_pd(Y, _mm256_permute4x64_pd(Y, ctl));
            const __m256d dz = _mm256_sub_pd(Z, _mm256_permute4x64_pd(Z, ctl));
            const __m256d d2 = _mm256_fmadd_pd(dx, dx,
                                   _mm256_fmadd_pd(dy, dy, _mm256_mul_pd(dz, dz)));
            const __m256d mag = _mm256_andnot_pd(self[i], inv_cube_dt(d2));
            const __m256d sx = _mm256_mul_pd(dx, mass[i]);
            const __m256d sy = _mm256_mul_pd(dy, mass[i]);
            const __m256d sz = _mm256_mul_pd(dz, mass[i]);
            AX = _mm256_fnmadd_pd(sx, mag, AX);
            AY = _mm256_fnmadd_pd(sy, mag, AY);
            AZ = _mm256_fnmadd_pd(sz, mag, AZ);
        }
        VX = _mm256_add_pd(VX, AX);
        VY = _mm256_add_pd(VY, AY);
        VZ = _mm256_add_pd(VZ, AZ);
        X = _mm256_fmadd_pd(ddt, VX, X);
        Y = _mm256_fmadd_pd(ddt, VY, Y);
        Z = _mm256_fmadd_pd(ddt, VZ, Z);
    }

    alignas(32) double x[4], y[4], z[4], vx[4], vy[4], vz[4];
    _mm256_store_pd(x, X); _mm256_store_pd(y, Y); _mm256_store_pd(z, Z);
    _mm256_store_pd(vx, VX); _mm256_store_pd(vy, VY); _mm256_store_pd(vz, VZ);
    e = energy(x[0], x[1], x[2], x[3], y[0], y[1], y[2], y[3], z[0], z[1], z[2], z[3],
               vx[0], vx[1], vx[2], vx[3], vy[0], vy[1], vy[2], vy[3], vz[0], vz[1], vz[2], vz[3]);
#else
    double x[4] = {0, 2, 4, 6}, y[4] = {0, 1, -1, 0.5}, z[4] = {0, 0.1, -0.1, 0.2};
    double vx[4] = {0, 0.01, -0.01, 0.02}, vy[4] = {0, 0.005, -0.005, 0.001};
    double vz[4] = {0, 0.001, -0.001, 0.0005};
    const double m[4] = {M0, M1, M2, M3};
    for (int s = 0; s < STEPS; ++s) {
        for (int i = 0; i < 4; ++i)
            for (int j = i + 1; j < 4; ++j) {
                const double dx = x[i] - x[j], dy = y[i] - y[j], dz = z[i] - z[j];
                const double d2 = dx * dx + dy * dy + dz * dz;
                const double d = std::sqrt(d2);
                const double mag = DT / (d2 * d);
                vx[i] -= dx * m[j] * mag; vy[i] -= dy * m[j] * mag; vz[i] -= dz * m[j] * mag;
                vx[j] += dx * m[i] * mag; vy[j] += dy * m[i] * mag; vz[j] += dz * m[i] * mag;
            }
        for (int i = 0; i < 4; ++i) {
            x[i] += DT * vx[i]; y[i] += DT * vy[i]; z[i] += DT * vz[i];
        }
    }
    e = energy(x[0], x[1], x[2], x[3], y[0], y[1], y[2], y[3], z[0], z[1], z[2], z[3],
               vx[0], vx[1], vx[2], vx[3], vy[0], vy[1], vy[2], vy[3], vz[0], vz[1], vz[2], vz[3]);
#endif

    std::printf("%.9f\n", e);
    return 0;
}
