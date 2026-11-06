/* Best-of integer matmul, N = 384.  Expected checksum 326316140.
 *
 * The reference is a plain i-j-k int matmul, then sums every even index of
 * C.  Here:
 *   - A and B entries are 0..9 / 0..6, and each C entry is at most 20736,
 *     so 16-bit products and 32-bit accumulators are exact.
 *   - B is pre-packed into 32-bit words holding b[2t][j] (low half) and
 *     b[2t+1][j] (high half); the matching A pair is broadcast, and
 *     vpmaddwd(a_pair, b_pair) adds both products into one int32 lane.
 *     8 columns per vector, four vectors (32 columns) per register block.
 *   - Rows are distributed with OpenMP.
 * The arithmetic is exact integer arithmetic, so the checksum is identical.
 */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <omp.h>
#ifdef __AVX2__
#include <immintrin.h>
#endif

#define N 384

int main(void)
{
    const size_t nn = (size_t)N * N;
    int16_t *a16 = (int16_t *)malloc(nn * sizeof(int16_t));
    int16_t *b16 = (int16_t *)malloc(nn * sizeof(int16_t));
    int     *c   = (int *)malloc(nn * sizeof(int));
    uint32_t *bp = (uint32_t *)malloc((size_t)(N / 2) * N * sizeof(uint32_t));
    uint32_t *ap = (uint32_t *)malloc((size_t)N * (N / 2) * sizeof(uint32_t));
    if (!a16 || !b16 || !c || !bp || !ap) return 1;

    for (int i = 0; i < N; i++) {
        for (int j = 0; j < N; j++) {
            a16[(size_t)i * N + j] = (int16_t)((i + j) % 10);
            b16[(size_t)i * N + j] = (int16_t)((i * j) % 7);
        }
    }
    /* pack B columns by pairs of rows: low = b[2t][j], high = b[2t+1][j] */
    for (int t = 0; t < N / 2; t++) {
        for (int j = 0; j < N; j++) {
            const uint16_t lo = (uint16_t)b16[(size_t)(2 * t) * N + j];
            const uint16_t hi = (uint16_t)b16[(size_t)(2 * t + 1) * N + j];
            bp[(size_t)t * N + j] = (uint32_t)lo | ((uint32_t)hi << 16);
        }
    }
    /* pack A rows by pairs of columns */
    for (int i = 0; i < N; i++) {
        for (int t = 0; t < N / 2; t++) {
            const uint16_t lo = (uint16_t)a16[(size_t)i * N + 2 * t];
            const uint16_t hi = (uint16_t)a16[(size_t)i * N + 2 * t + 1];
            ap[(size_t)i * (N / 2) + t] = (uint32_t)lo | ((uint32_t)hi << 16);
        }
    }

#ifdef __AVX2__
#pragma omp parallel for schedule(static) num_threads(8)
    for (int i = 0; i < N; i++) {
        const uint32_t *restrict api = ap + (size_t)i * (N / 2);
        int *restrict ci = c + (size_t)i * N;

        for (int jb = 0; jb < N; jb += 32) {
            __m256i acc0 = _mm256_setzero_si256();
            __m256i acc1 = _mm256_setzero_si256();
            __m256i acc2 = _mm256_setzero_si256();
            __m256i acc3 = _mm256_setzero_si256();
            for (int t = 0; t < N / 2; t++) {
                const __m256i av = _mm256_set1_epi32((int)api[t]);
                const uint32_t *bp0 = bp + (size_t)t * N + jb;
                acc0 = _mm256_add_epi32(acc0, _mm256_madd_epi16(
                    av, _mm256_loadu_si256((const __m256i *)(bp0))));
                acc1 = _mm256_add_epi32(acc1, _mm256_madd_epi16(
                    av, _mm256_loadu_si256((const __m256i *)(bp0 + 8))));
                acc2 = _mm256_add_epi32(acc2, _mm256_madd_epi16(
                    av, _mm256_loadu_si256((const __m256i *)(bp0 + 16))));
                acc3 = _mm256_add_epi32(acc3, _mm256_madd_epi16(
                    av, _mm256_loadu_si256((const __m256i *)(bp0 + 24))));
            }
            _mm256_storeu_si256((__m256i *)(ci + jb),      acc0);
            _mm256_storeu_si256((__m256i *)(ci + jb + 8),  acc1);
            _mm256_storeu_si256((__m256i *)(ci + jb + 16), acc2);
            _mm256_storeu_si256((__m256i *)(ci + jb + 24), acc3);
        }
    }
#else
    for (int i = 0; i < N; i++) {
        for (int j = 0; j < N; j++) {
            int s = 0;
            for (int k = 0; k < N; k++)
                s += (int)a16[(size_t)i * N + k] * (int)b16[(size_t)k * N + j];
            c[(size_t)i * N + j] = s;
        }
    }
#endif

    long long total = 0;
    for (size_t x = 0; x < nn; x += 2) total += c[x];
    printf("%lld\n", total);

    free(a16); free(b16); free(c); free(bp); free(ap);
    return 0;
}
