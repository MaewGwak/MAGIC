/* Fannkuch-redux, n = 10.
 *
 * 90 independent streams: the top-level block fixes the last element (10
 * choices) and the sub-block fixes the eighth (9 choices); 8! is even, so each
 * stream starts on an even global index and the checksum sign simply
 * alternates inside the stream.  Each stream runs the standard odometer over
 * the first eight positions.
 *
 * Values are 0..9, so the whole permutation lives in one 16-byte SSE register
 * and every pancake flip is a single PSHUFB prefix reversal with a mask from a
 * 16-entry table - no per-element swaps.  Parallel over streams with OpenMP;
 * the integer checksum and the max are order-independent and exact.
 *
 * Expected: 73196 and 38.
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <immintrin.h>
#include <omp.h>

#define N 10
#define M 8
#define SUBS 9
#define STREAMS 90


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
    static uint8_t revmask[16][16] __attribute__((aligned(16)));
    for (int k = 0; k < 16; k++)
        for (int i = 0; i < 16; i++)
            revmask[k][i] = (uint8_t)(i <= k ? k - i : i);   /* reverse 0..k */

    long long sums[STREAMS];
    int maxes[STREAMS];

    #pragma omp parallel for schedule(dynamic, 1)
    for (int s = 0; s < STREAMS; s++) {
        const int b1 = s / SUBS;
        const int b2 = s % SUBS;

        uint8_t p1[16] __attribute__((aligned(16)));
        uint8_t tmp[16] __attribute__((aligned(16)));
        int cnt[16];
        memset(p1, 0, 16);
        for (int i = 0; i < N; i++) p1[i] = (uint8_t)i;

        /* rotate the whole array by b1, then the first M+1 by b2 */
        for (int i = 0; i < N; i++) tmp[i] = p1[(i + b1) % N];
        memcpy(p1, tmp, N);
        for (int i = 0; i <= M; i++) tmp[i] = p1[(i + b2) % (M + 1)];
        memcpy(p1, tmp, M + 1);

        int r = M;
        int mx = 0;
        int parity = 0;
        long long sum = 0;

        for (;;) {
            while (r != 1) { cnt[r - 1] = r; r--; }

            __m128i v = _mm_load_si128((const __m128i *)p1);
            int flips = 0;
            int k = _mm_cvtsi128_si32(v) & 0xFF;
            while (k) {
                v = _mm_shuffle_epi8(v, _mm_load_si128((const __m128i *)revmask[k]));
                flips++;
                k = _mm_cvtsi128_si32(v) & 0xFF;
            }
            if (flips > mx) mx = flips;
            if (parity) sum -= flips; else sum += flips;
            parity ^= 1;

            for (;;) {
                if (r == M) goto done;
                uint8_t p0 = p1[0];
                for (int k2 = 0; k2 < r; k2++) p1[k2] = p1[k2 + 1];
                p1[r] = p0;
                if (--cnt[r] > 0) break;
                r++;
            }
        }
    done:
        sums[s] = sum;
        maxes[s] = mx;
    }

    long long checksum = 0;
    int mxall = 0;
    for (int s = 0; s < STREAMS; s++) {
        checksum += sums[s];
        if (maxes[s] > mxall) mxall = maxes[s];
    }
    printf("%lld\n%d\n", checksum, mxall);
    return 0;
}
