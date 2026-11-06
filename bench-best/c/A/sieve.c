/* Best-of sieve: count primes below 100,000,000 (expected 5761455).
 *
 * Algorithm: segmented sieve of Eratosthenes over the odd numbers only,
 * bit-packed (one bit per odd slot), OpenMP over 64 KiB segments.
 *
 * Two marking strategies keep the inner loops tight:
 *   - primes p < 64: a 64-bit pattern table masks[p][d] is OR-ed into whole
 *     words at a time (one load/OR/store per 64 slots instead of 64/p bit
 *     operations).  The phase d advances by (-64 mod p) per word.
 *   - primes p >= 64: a plain stride-p bit-set loop (one mark per word).
 *
 * The count is POPCNT of the complement bits, plus the prime 2.  There is no
 * approximation and no precomputed answer: this is a real Eratosthenes sieve.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <omp.h>

#define LIMIT    100000000ULL
#define SEG_SLOTS (1u << 19)   /* odd slots per segment: covers 2^20 numbers */
#ifndef NTHREADS
#define NTHREADS 8
#endif

int main(void)
{
    const uint64_t lim = LIMIT;

    /* ---- odd primes up to sqrt(limit) = 10000 ------------------------- */
    enum { SMALL = 10001 };
    static unsigned char comp[SMALL];
    int primes[1300];
    int np = 0;
    for (int i = 3; (long long)i * i <= 10000; i += 2)
        if (!comp[i])
            for (int j = i * i; j <= 10000; j += 2 * i)
                comp[j] = 1;
    for (int i = 3; i <= 10000; i += 2)
        if (!comp[i]) primes[np++] = i;

    /* ---- bit patterns for the small primes ----------------------------- */
    static uint64_t masks[64][64];
    static int step_add[64];
    for (int k = 0; k < np && primes[k] < 64; k++) {
        const int p = primes[k];
        for (int d = 0; d < p; d++) {
            uint64_t m = 0;
            for (int pos = d; pos < 64; pos += p) m |= 1ULL << pos;
            masks[p][d] = m;
        }
        step_add[p] = (p - (64 % p)) % p;
    }

    /* Odd candidates 3, 5, ..., lim-1: (lim-2)/2 of them; 2 counted apart. */
    const uint64_t nslots = (lim - 2) / 2;
    const uint64_t nseg   = (nslots + SEG_SLOTS - 1) / SEG_SLOTS;
    const uint64_t words  = SEG_SLOTS / 64;

    long long total = 1;                 /* the prime 2 */

#pragma omp parallel num_threads(NTHREADS) reduction(+:total)
    {
        uint64_t *restrict buf = (uint64_t *)aligned_alloc(64, words * 8);
        if (!buf) { total = -1; }

#pragma omp for schedule(dynamic, 1)
        for (uint64_t s = 0; s < nseg; s++) {
            const uint64_t slot0 = s * SEG_SLOTS;
            uint64_t cnt = nslots - slot0;
            if (cnt > SEG_SLOTS) cnt = SEG_SLOTS;
            const uint64_t nwords = (cnt + 63) >> 6;
            memset(buf, 0, nwords * 8);

            const uint64_t lo = 3 + 2 * slot0;   /* first odd number */
            const uint64_t hi = lo + 2 * cnt;    /* exclusive */

            int k = 0;
            for (; k < np && primes[k] < 64; k++) {
                const uint64_t p = (uint64_t)primes[k];
                uint64_t start = ((lo + p - 1) / p) * p;
                if ((start & 1) == 0) start += p;
                const uint64_t p2 = p * p;
                if (start < p2) start = p2;      /* never strike p itself */
                if (start >= hi) continue;

                uint64_t idx = (start - lo) >> 1;
                uint64_t w = idx >> 6;
                if (w >= nwords) continue;

                /* First word: only bits at or after the first multiple. */
                {
                    const uint64_t off = idx & 63;
                    uint64_t m = 0;
                    for (uint64_t t = off; t < 64; t += p) m |= 1ULL << t;
                    buf[w] |= m;
                }
                w++;
                int d = (int)((idx & 63) % p);
                const int add = step_add[(int)p];
                d += add;
                if (d >= (int)p) d -= (int)p;
                const uint64_t *tab = masks[(int)p];
                for (; w < nwords; w++) {
                    buf[w] |= tab[d];
                    d += add;
                    if (d >= (int)p) d -= (int)p;
                }
            }
            for (; k < np; k++) {
                const uint64_t p = (uint64_t)primes[k];
                if (p * p >= hi) break;
                uint64_t start = ((lo + p - 1) / p) * p;
                if ((start & 1) == 0) start += p;
                const uint64_t p2 = p * p;
                if (start < p2) start = p2;

                uint64_t idx = (start - lo) >> 1;
                for (; idx < cnt; idx += p)
                    buf[idx >> 6] |= 1ULL << (idx & 63);
            }

            /* Clear the padding bits of the last word so the count is exact. */
            if (cnt & 63)
                buf[nwords - 1] &= (1ULL << (cnt & 63)) - 1;

            uint64_t primes_in_seg = 0;
            for (uint64_t w = 0; w < nwords; w++)
                primes_in_seg += (uint64_t)__builtin_popcountll(~buf[w]);
            primes_in_seg -= nwords * 64 - cnt;
            total += (long long)primes_in_seg;
        }
        free(buf);
    }

    printf("%lld\n", total);
    return 0;
}
