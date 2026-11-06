// Best-of C++: ray tracer, 1536 x 1536, 5 spheres.
//
// Workload (bench-suite/raytrace_c.c): for every pixel build the primary ray,
// intersect it with 5 spheres, keep the nearest positive hit, shade it with a
// point light and accumulate the brightness; print the total with 3 decimals.
//
// Optimisations (exact same ray/sphere maths, no approximation):
//   * OpenMP over rows: the image is embarrassingly parallel; each thread keeps
//     a private partial sum (the printed 3-decimal value is stable under any
//     summation order: different orders move it by ~4e-9 on 3.36e5).
//   * AVX2 doubles, 4 pixels per iteration, fully branchless.  A 4-pixel group
//     costs 5 packed sqrt for the sphere discriminants, 2 packed sqrt for the
//     shading, and 3 packed divides (len, n, light vector - each reciprocal is
//     formed once and multiplied, and the u scale is a multiply by 1/768, which
//     removes 5 packed divides per group).  disc is clamped to 0 before sqrt
//     and the hit is selected with compare/blend masks.  The winning sphere's
//     centre is blended in (5 cheap blends) instead of a scalar gather.
//   * No-hit lanes still execute the (finite) shading arithmetic with
//     best = 1e9 and are then blended to 0, matching the reference's
//     "if (hit >= 0)" exactly.
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cmath>
#include <algorithm>
#include <immintrin.h>
#include <omp.h>


// This kernel is FP-pipe bound (sqrt/div) and SMT siblings share the same FP
// units, so one thread per *physical* core beats one per logical CPU on this
// APU (it also avoids the power/boost penalty of a fully occupied SMT pair).
// Fallback: all logical CPUs when /proc/cpuinfo does not report "cpu cores".
static int physical_cores() {
    const int logical = omp_get_num_procs();
    int cores = 0;
    if (FILE *f = std::fopen("/proc/cpuinfo", "r")) {
        char line[256];
        while (std::fgets(line, sizeof line, f)) {
            if (std::strncmp(line, "cpu cores", 9) == 0) {
                if (const char *c = std::strchr(line, ':')) cores = std::atoi(c + 1);
                break;
            }
        }
        std::fclose(f);
    }
    if (cores > 0) return cores < logical ? cores : logical;
    return logical;
}

int main() {
    const int W = 1536, H = 1536, S = 5;
    const double cx[S] = {0.0, 1.5, -1.2, 2.5, -2.0};
    const double cy[S] = {0.0, 0.3, -0.6, -0.8, 1.2};
    const double cz[S] = {3.0, 3.2, 2.8, 4.0, 3.5};
    const double rr[S] = {1.0, 0.6, 0.7, 0.5, 0.4};
    const double lx = 5.0, ly = 5.0, lz = 0.0;

    double total = 0.0;
    const int nthreads = physical_cores();

#pragma omp parallel for reduction(+ : total) schedule(static) num_threads(nthreads)
    for (int y = 0; y < H; y++) {
        const double v = (y - H / 2.0) / (H / 2.0);
        const __m256d vv = _mm256_set1_pd(v);
        const __m256d dz0 = _mm256_set1_pd(1.5);
        __m256d rowsum = _mm256_setzero_pd();

        for (int x = 0; x < W; x += 4) {
            const __m256d uu = _mm256_set_pd(x + 3 - W / 2.0, x + 2 - W / 2.0,
                                             x + 1 - W / 2.0, x - W / 2.0);
            __m256d dx = _mm256_mul_pd(uu, _mm256_set1_pd(1.0 / (W / 2.0)));
            __m256d dy = vv;
            __m256d dz = dz0;
            __m256d len = _mm256_sqrt_pd(
                _mm256_add_pd(_mm256_add_pd(_mm256_mul_pd(dx, dx), _mm256_mul_pd(dy, dy)),
                              _mm256_mul_pd(dz, dz)));
            const __m256d invlen = _mm256_div_pd(_mm256_set1_pd(1.0), len);
            dx = _mm256_mul_pd(dx, invlen);
            dy = _mm256_mul_pd(dy, invlen);
            dz = _mm256_mul_pd(dz, invlen);

            __m256d best = _mm256_set1_pd(1e9);
            __m256d selx = _mm256_setzero_pd();
            __m256d sely = _mm256_setzero_pd();
            __m256d selz = _mm256_setzero_pd();
            __m256d hit = _mm256_setzero_pd();

            for (int s = 0; s < S; s++) {
                const __m256d ox = _mm256_set1_pd(-cx[s]);
                const __m256d oy = _mm256_set1_pd(-cy[s]);
                const __m256d oz = _mm256_set1_pd(-cz[s]);
                const __m256d cc = _mm256_set1_pd(
                    cx[s] * cx[s] + cy[s] * cy[s] + cz[s] * cz[s] - rr[s] * rr[s]);
                const __m256d b = _mm256_add_pd(
                    _mm256_add_pd(_mm256_mul_pd(ox, dx), _mm256_mul_pd(oy, dy)),
                    _mm256_mul_pd(oz, dz));
                __m256d disc = _mm256_sub_pd(_mm256_mul_pd(b, b), cc);
                __m256d t = _mm256_sub_pd(
                    _mm256_sub_pd(_mm256_setzero_pd(), b),
                    _mm256_sqrt_pd(_mm256_max_pd(disc, _mm256_setzero_pd())));
                __m256d m = _mm256_and_pd(
                    _mm256_cmp_pd(disc, _mm256_setzero_pd(), _CMP_GT_OQ),
                    _mm256_and_pd(_mm256_cmp_pd(t, _mm256_setzero_pd(), _CMP_GT_OQ),
                                  _mm256_cmp_pd(t, best, _CMP_LT_OQ)));
                best = _mm256_blendv_pd(best, t, m);
                selx = _mm256_blendv_pd(selx, _mm256_set1_pd(cx[s]), m);
                sely = _mm256_blendv_pd(sely, _mm256_set1_pd(cy[s]), m);
                selz = _mm256_blendv_pd(selz, _mm256_set1_pd(cz[s]), m);
                hit = _mm256_or_pd(hit, m);
            }

            const __m256d px = _mm256_mul_pd(dx, best);
            const __m256d py = _mm256_mul_pd(dy, best);
            const __m256d pz = _mm256_mul_pd(dz, best);
            __m256d nx = _mm256_sub_pd(px, selx);
            __m256d ny = _mm256_sub_pd(py, sely);
            __m256d nz = _mm256_sub_pd(pz, selz);
            const __m256d nl = _mm256_sqrt_pd(_mm256_add_pd(
                _mm256_add_pd(_mm256_mul_pd(nx, nx), _mm256_mul_pd(ny, ny)),
                _mm256_mul_pd(nz, nz)));
            const __m256d invnl = _mm256_div_pd(_mm256_set1_pd(1.0), nl);
            nx = _mm256_mul_pd(nx, invnl);
            ny = _mm256_mul_pd(ny, invnl);
            nz = _mm256_mul_pd(nz, invnl);
            const __m256d ldx = _mm256_sub_pd(_mm256_set1_pd(lx), px);
            const __m256d ldy = _mm256_sub_pd(_mm256_set1_pd(ly), py);
            const __m256d ldz = _mm256_sub_pd(_mm256_set1_pd(lz), pz);
            const __m256d ll = _mm256_sqrt_pd(_mm256_add_pd(
                _mm256_add_pd(_mm256_mul_pd(ldx, ldx), _mm256_mul_pd(ldy, ldy)),
                _mm256_mul_pd(ldz, ldz)));
            __m256d d = _mm256_mul_pd(
                _mm256_add_pd(_mm256_add_pd(_mm256_mul_pd(nx, ldx), _mm256_mul_pd(ny, ldy)),
                              _mm256_mul_pd(nz, ldz)),
                _mm256_div_pd(_mm256_set1_pd(1.0), ll));
            d = _mm256_max_pd(d, _mm256_setzero_pd());
            rowsum = _mm256_add_pd(rowsum, _mm256_blendv_pd(_mm256_setzero_pd(), d, hit));
        }

        const __m128d lo = _mm256_castpd256_pd128(rowsum);
        const __m128d hi = _mm256_extractf128_pd(rowsum, 1);
        const __m128d s2 = _mm_add_pd(lo, hi);
        total += _mm_cvtsd_f64(s2) + _mm_cvtsd_f64(_mm_unpackhi_pd(s2, s2));
    }

    std::printf("%.3f\n", total);
    return 0;
}
