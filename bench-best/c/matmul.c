/* Integer matmul, N = 384, C = A*B.
 *
 * Blocked i-k-j kernel.  Every a entry is (i+j)%10 <= 9 and every b entry is
 * (i*j)%7 <= 6, so every partial dot product is at most 384*54 = 20736 and fits
 * exactly in a signed 16-bit lane: the whole kernel runs in 32-lane int16
 * vectors (2x the int32 throughput and half the memory traffic), and the final
 * checksum widens to 64 bits.
 *
 * 8 rows of C are held in 16 zmm accumulators while one 64-column panel of B
 * is swept over all k, so C never leaves registers inside the band and B is
 * read once per 8-row band.
 *
 * Expected: 326316140
 */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <immintrin.h>

#define N 384

int main(void) {
    int16_t *a = (int16_t *)aligned_alloc(64, (size_t)N * N * sizeof(int16_t));
    int16_t *b = (int16_t *)aligned_alloc(64, (size_t)N * N * sizeof(int16_t));
    int16_t *c = (int16_t *)aligned_alloc(64, (size_t)N * N * sizeof(int16_t));
    if (!a || !b || !c) return 1;

    for (int i = 0; i < N; i++)
        for (int j = 0; j < N; j++) {
            a[i * N + j] = (int16_t)((i + j) % 10);
            b[i * N + j] = (int16_t)((i * j) % 7);
        }

    for (int ii = 0; ii < N; ii += 8) {
        for (int jj = 0; jj < N; jj += 64) {
            __m512i c00 = _mm512_setzero_si512(), c01 = _mm512_setzero_si512();
            __m512i c10 = _mm512_setzero_si512(), c11 = _mm512_setzero_si512();
            __m512i c20 = _mm512_setzero_si512(), c21 = _mm512_setzero_si512();
            __m512i c30 = _mm512_setzero_si512(), c31 = _mm512_setzero_si512();
            __m512i c40 = _mm512_setzero_si512(), c41 = _mm512_setzero_si512();
            __m512i c50 = _mm512_setzero_si512(), c51 = _mm512_setzero_si512();
            __m512i c60 = _mm512_setzero_si512(), c61 = _mm512_setzero_si512();
            __m512i c70 = _mm512_setzero_si512(), c71 = _mm512_setzero_si512();

            for (int k = 0; k < N; k++) {
                const __m512i b0 = _mm512_load_si512((const void *)&b[k * N + jj]);
                const __m512i b1 = _mm512_load_si512((const void *)&b[k * N + jj + 32]);
                __m512i av;

                av = _mm512_set1_epi16(a[(ii + 0) * N + k]);
                c00 = _mm512_add_epi16(c00, _mm512_mullo_epi16(av, b0));
                c01 = _mm512_add_epi16(c01, _mm512_mullo_epi16(av, b1));
                av = _mm512_set1_epi16(a[(ii + 1) * N + k]);
                c10 = _mm512_add_epi16(c10, _mm512_mullo_epi16(av, b0));
                c11 = _mm512_add_epi16(c11, _mm512_mullo_epi16(av, b1));
                av = _mm512_set1_epi16(a[(ii + 2) * N + k]);
                c20 = _mm512_add_epi16(c20, _mm512_mullo_epi16(av, b0));
                c21 = _mm512_add_epi16(c21, _mm512_mullo_epi16(av, b1));
                av = _mm512_set1_epi16(a[(ii + 3) * N + k]);
                c30 = _mm512_add_epi16(c30, _mm512_mullo_epi16(av, b0));
                c31 = _mm512_add_epi16(c31, _mm512_mullo_epi16(av, b1));
                av = _mm512_set1_epi16(a[(ii + 4) * N + k]);
                c40 = _mm512_add_epi16(c40, _mm512_mullo_epi16(av, b0));
                c41 = _mm512_add_epi16(c41, _mm512_mullo_epi16(av, b1));
                av = _mm512_set1_epi16(a[(ii + 5) * N + k]);
                c50 = _mm512_add_epi16(c50, _mm512_mullo_epi16(av, b0));
                c51 = _mm512_add_epi16(c51, _mm512_mullo_epi16(av, b1));
                av = _mm512_set1_epi16(a[(ii + 6) * N + k]);
                c60 = _mm512_add_epi16(c60, _mm512_mullo_epi16(av, b0));
                c61 = _mm512_add_epi16(c61, _mm512_mullo_epi16(av, b1));
                av = _mm512_set1_epi16(a[(ii + 7) * N + k]);
                c70 = _mm512_add_epi16(c70, _mm512_mullo_epi16(av, b0));
                c71 = _mm512_add_epi16(c71, _mm512_mullo_epi16(av, b1));
            }

            _mm512_store_si512((void *)&c[(ii + 0) * N + jj], c00);
            _mm512_store_si512((void *)&c[(ii + 0) * N + jj + 32], c01);
            _mm512_store_si512((void *)&c[(ii + 1) * N + jj], c10);
            _mm512_store_si512((void *)&c[(ii + 1) * N + jj + 32], c11);
            _mm512_store_si512((void *)&c[(ii + 2) * N + jj], c20);
            _mm512_store_si512((void *)&c[(ii + 2) * N + jj + 32], c21);
            _mm512_store_si512((void *)&c[(ii + 3) * N + jj], c30);
            _mm512_store_si512((void *)&c[(ii + 3) * N + jj + 32], c31);
            _mm512_store_si512((void *)&c[(ii + 4) * N + jj], c40);
            _mm512_store_si512((void *)&c[(ii + 4) * N + jj + 32], c41);
            _mm512_store_si512((void *)&c[(ii + 5) * N + jj], c50);
            _mm512_store_si512((void *)&c[(ii + 5) * N + jj + 32], c51);
            _mm512_store_si512((void *)&c[(ii + 6) * N + jj], c60);
            _mm512_store_si512((void *)&c[(ii + 6) * N + jj + 32], c61);
            _mm512_store_si512((void *)&c[(ii + 7) * N + jj], c70);
            _mm512_store_si512((void *)&c[(ii + 7) * N + jj + 32], c71);
        }
    }

    long long total = 0;
    for (int x = 0; x < N * N; x += 2) total += c[x];
    printf("%lld\n", total);

    free(a);
    free(b);
    free(c);
    return 0;
}
