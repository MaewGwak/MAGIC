// ray tracer 1536^2 -> 336141.941.
//
// Same scene and the same per-pixel math as the reference, but:
//   * 4 pixels at a time with AVX2 (the scene tests are branch-free SIMD),
//   * the ray/sphere test uses the reduced quadratic  b = o.d,
//     disc = b^2 - (|o|^2 - r^2) with o = -centre, so the inner test is
//     two FMAs plus a compare; sqrt(disc) is only executed when at least one
//     of the four lanes can hit that sphere,
//   * shading is also vectorised: the per-lane hit centre/radius are picked
//     with blends and 4 lanes are normalised and lit in parallel (the normal
//     is divided by the known radius instead of recomputing |p-c|, which is
//     algebraically identical),
//   * OpenMP over scanlines with a reduction.
//
// Floating point: all arithmetic is double; only the association order of the
// dot products (FMA contraction) and of the final summation (parallel
// reduction) differ from the reference, at the 1e-15 relative level.  That is
// far inside the 1e-9 relative tolerance and the printed %.3f is identical
// (336141.941).
#include <cstdio>
#include <cmath>
#include <cstdint>
#include <algorithm>

#ifdef _OPENMP
#include <omp.h>
#endif

#if defined(__AVX2__)
#include <immintrin.h>
#endif

namespace {

constexpr int W = 1536;
constexpr int H = 1536;
constexpr int S = 5;
constexpr double HALF = 768.0;

const double CX[S] = {0.0, 1.5, -1.2, 2.5, -2.0};
const double CY[S] = {0.0, 0.3, -0.6, -0.8, 1.2};
const double CZ[S] = {3.0, 3.2, 2.8, 4.0, 3.5};
const double RR[S] = {1.0, 0.6, 0.7, 0.5, 0.4};
const double LX = 5.0, LY = 5.0;

inline double shade_scalar(int h, double dx, double dy, double dz, double t) {
    const double px = dx * t, py = dy * t, pz = dz * t;
    double nx = px - CX[h], ny = py - CY[h], nz = pz - CZ[h];
    const double invr = 1.0 / RR[h];
    nx *= invr; ny *= invr; nz *= invr;
    const double ldx = LX - px, ldy = LY - py, ldz = -pz;
    const double ll = std::sqrt(ldx * ldx + ldy * ldy + ldz * ldz);
    double d = (nx * ldx + ny * ldy + nz * ldz) / ll;
    return d < 0.0 ? 0.0 : d;
}

}  // namespace

int main() {
    double total = 0.0;

#ifdef _OPENMP
    // 8 = physical core count of the benchmark host; SMT siblings do not help
    // this divider-heavy kernel and 16 threads are slower in practice.
    omp_set_num_threads(std::min(8, omp_get_num_procs()));
#endif

#if defined(__AVX2__)
    __m256d ox[S], oy[S], oz[S], c0[S], invr[S];
    for (int s = 0; s < S; ++s) {
        ox[s] = _mm256_set1_pd(-CX[s]);
        oy[s] = _mm256_set1_pd(-CY[s]);
        oz[s] = _mm256_set1_pd(-CZ[s]);
        c0[s] = _mm256_set1_pd(CX[s] * CX[s] + CY[s] * CY[s] + CZ[s] * CZ[s] - RR[s] * RR[s]);
        invr[s] = _mm256_set1_pd(1.0 / RR[s]);
    }
    const __m256d zero = _mm256_setzero_pd();
    const __m256d one = _mm256_set1_pd(1.0);
    const __m256d huge = _mm256_set1_pd(1e9);
    const __m256d five = _mm256_set1_pd(LX);
    const __m256d neg168 = _mm256_set1_pd(1.0 / 768.0);

#pragma omp parallel for schedule(dynamic, 1) reduction(+ : total)
    for (int y = 0; y < H; ++y) {
        const double v = (y - HALF) / HALF;
        const __m256d dy0 = _mm256_set1_pd(v);
        const __m256d dz0 = _mm256_set1_pd(1.5);
        const __m256d dyv = dy0, dzv = dz0;
        __m256d rowacc = _mm256_setzero_pd();

        for (int x = 0; x < W; x += 4) {
            const __m256d xv = _mm256_set_pd(x + 3.0, x + 2.0, x + 1.0, (double)x);
            __m256d dx = _mm256_fmadd_pd(xv, neg168, _mm256_set1_pd(-1.0));
            __m256d dy = dyv, dz = dzv;
            const __m256d len2 = _mm256_fmadd_pd(dx, dx, _mm256_fmadd_pd(dy, dy,
                                                                _mm256_mul_pd(dz, dz)));
            const __m256d inv = _mm256_div_pd(one, _mm256_sqrt_pd(len2));
            dx = _mm256_mul_pd(dx, inv);
            dy = _mm256_mul_pd(dy, inv);
            dz = _mm256_mul_pd(dz, inv);

            __m256d best = huge;
            __m256d hit = _mm256_set1_pd(-1.0);
            for (int s = 0; s < S; ++s) {
                const __m256d b = _mm256_fmadd_pd(ox[s], dx,
                                       _mm256_fmadd_pd(oy[s], dy, _mm256_mul_pd(oz[s], dz)));
                const __m256d disc = _mm256_fmsub_pd(b, b, c0[s]);
                const __m256d dmask = _mm256_cmp_pd(disc, zero, _CMP_GT_OQ);
                if (_mm256_movemask_pd(dmask) == 0) continue;
                const __m256d t = _mm256_sub_pd(_mm256_sub_pd(zero, _mm256_sqrt_pd(disc)), b);
                __m256d m = _mm256_and_pd(dmask, _mm256_cmp_pd(t, zero, _CMP_GT_OQ));
                m = _mm256_and_pd(m, _mm256_cmp_pd(t, best, _CMP_LT_OQ));
                best = _mm256_blendv_pd(best, t, m);
                hit = _mm256_blendv_pd(hit, _mm256_set1_pd((double)s), m);
            }

            // ---- shading, 4 lanes at once --------------------------------
            // Select the hit centre / inverse radius per lane; miss lanes are
            // masked out at the end.
            __m256d cx = _mm256_set1_pd(CX[0]), cy = _mm256_set1_pd(CY[0]);
            __m256d cz = _mm256_set1_pd(CZ[0]), iv = invr[0];
            for (int s = 1; s < S; ++s) {
                const __m256d ms = _mm256_cmp_pd(hit, _mm256_set1_pd((double)s), _CMP_EQ_OQ);
                cx = _mm256_blendv_pd(cx, _mm256_set1_pd(CX[s]), ms);
                cy = _mm256_blendv_pd(cy, _mm256_set1_pd(CY[s]), ms);
                cz = _mm256_blendv_pd(cz, _mm256_set1_pd(CZ[s]), ms);
                iv = _mm256_blendv_pd(iv, invr[s], ms);
            }
            const __m256d px = _mm256_mul_pd(dx, best);
            const __m256d py = _mm256_mul_pd(dy, best);
            const __m256d pz = _mm256_mul_pd(dz, best);
            const __m256d nx = _mm256_mul_pd(_mm256_sub_pd(px, cx), iv);
            const __m256d ny = _mm256_mul_pd(_mm256_sub_pd(py, cy), iv);
            const __m256d nz = _mm256_mul_pd(_mm256_sub_pd(pz, cz), iv);
            const __m256d lx = _mm256_sub_pd(five, px);
            const __m256d ly = _mm256_sub_pd(five, py);
            const __m256d lz = _mm256_sub_pd(zero, pz);
            const __m256d dot = _mm256_fmadd_pd(nx, lx,
                                     _mm256_fmadd_pd(ny, ly, _mm256_mul_pd(nz, lz)));
            const __m256d ll2 = _mm256_fmadd_pd(lx, lx,
                                     _mm256_fmadd_pd(ly, ly, _mm256_mul_pd(lz, lz)));
            __m256d col = _mm256_div_pd(dot, _mm256_sqrt_pd(ll2));
            col = _mm256_max_pd(col, zero);
            const __m256d hitm = _mm256_cmp_pd(hit, _mm256_set1_pd(-0.5), _CMP_GT_OQ);
            rowacc = _mm256_add_pd(rowacc, _mm256_and_pd(col, hitm));
        }
        alignas(32) double tmp[4];
        _mm256_store_pd(tmp, rowacc);
        total += (tmp[0] + tmp[1]) + (tmp[2] + tmp[3]);
    }
#else
#pragma omp parallel for schedule(dynamic, 1) reduction(+ : total)
    for (int y = 0; y < H; ++y) {
        const double v = (y - HALF) / HALF;
        double row = 0.0;
        for (int x = 0; x < W; ++x) {
            double dx = (x - HALF) / HALF, dy = v, dz = 1.5;
            const double len = std::sqrt(dx * dx + dy * dy + dz * dz);
            dx /= len; dy /= len; dz /= len;
            double best = 1e9;
            int hit = -1;
            for (int s = 0; s < S; ++s) {
                const double ox = -CX[s], oy = -CY[s], oz = -CZ[s];
                const double b = ox * dx + oy * dy + oz * dz;
                const double c = ox * ox + oy * oy + oz * oz - RR[s] * RR[s];
                const double disc = b * b - c;
                if (disc > 0) {
                    const double t = -b - std::sqrt(disc);
                    if (t > 0 && t < best) { best = t; hit = s; }
                }
            }
            if (hit >= 0) row += shade_scalar(hit, dx, dy, dz, best);
        }
        total += row;
    }
#endif

    std::printf("%.3f\n", total);
    return 0;
}
