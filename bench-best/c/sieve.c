/* Sieve of Eratosthenes, primes below 100,000,000.
 *
 * A mod-30 wheel bitmap: one byte holds the 8 residues {1,7,11,13,17,19,23,29}
 * of a 30-number block, so the bitmap is N/30 = 3.33 MB (8x smaller than a
 * byte-per-number sieve) and 2, 3 and 5 need no marking at all.  For a prime p
 * every residue class advances by exactly p bytes in q-space (the block index),
 * so marking is eight arithmetic-progression sweeps with no divisions in the
 * inner loop.
 *
 * The q-range is split into one contiguous chunk per thread; the chunks are
 * disjoint regions of one shared bitmap, so there is no cross-thread traffic.
 * Each thread's 200 KB slice stays in L2.  Final count is a popcount pass.
 *
 * Expected: 5761455
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <omp.h>

#define N 100000000
#define QMAX (N / 30)          /* covers 0 .. 30*QMAX-1 */

static const int RES[8] = {1, 7, 11, 13, 17, 19, 23, 29};

static int inv_mod(int a, int p) {   /* a^-1 mod p, p prime, gcd(a,p)=1 */
    int t = 0, newt = 1, r = p, newr = a % p;
    while (newr) {
        int q = r / newr;
        int tmp = t - q * newt; t = newt; newt = tmp;
        tmp = r - q * newr;     r = newr; newr = tmp;
    }
    return t < 0 ? t + p : t;
}

static int is_prime_small(long long v, const int *pr, int np) {
    if (v < 2) return 0;
    if (v % 2 == 0) return v == 2;
    if (v % 3 == 0) return v == 3;
    if (v % 5 == 0) return v == 5;
    for (int i = 0; i < np; i++) {
        long long p = pr[i];
        if (p * p > v) return 1;
        if (v % p == 0) return v == p;
    }
    return 1;
}


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
    unsigned char *bm = (unsigned char *)malloc(QMAX);
    int *primes = (int *)malloc(1400 * sizeof(int));
    if (!bm || !primes) return 1;

    /* base primes 7 .. 10000 */
    static unsigned char small[10001];
    int np = 0;
    for (int i = 2; i <= 10000; i++)
        if (!small[i]) {
            if (i >= 7) primes[np++] = i;
            for (long long j = (long long)i * i; j <= 10000; j += i) small[j] = 1;
        }

    memset(bm, 0xFF, QMAX);

    const int nchunk = 32;
    #pragma omp parallel for schedule(static)
    for (int t = 0; t < nchunk; t++) {
        long long qlo = (long long)QMAX * t / nchunk;
        long long qhi = (long long)QMAX * (t + 1) / nchunk;
        long long hi_val = 30 * qhi + 29;

        for (int pi = 0; pi < np; pi++) {
            const long long p = primes[pi];
            if (p * p >= hi_val) break;
            const long long step = 30 * p;
            const int inv30 = inv_mod((int)(30 % p), (int)p);
            for (int k = 0; k < 8; k++) {
                const int r = RES[k];
                int c = (p - (long long)r * inv30 % p) % p;      /* q = c (mod p) */
                long long d = ((long long)c - qlo) % p;
                if (d < 0) d += p;
                long long q0 = qlo + d;
                long long v0 = 30 * q0 + r;
                if (v0 < p * p)
                    q0 += ((p * p - v0 + step - 1) / step) * p;
                const unsigned char mask = (unsigned char)~(1u << k);
                for (long long q = q0; q < qhi; q += p) bm[q] &= mask;
            }
        }
    }

    long long cnt = 0;
    #pragma omp parallel for reduction(+:cnt) schedule(static)
    for (long long q = 0; q < QMAX; q++)
        cnt += __builtin_popcount((unsigned)bm[q]);

    cnt += 3;   /* 2, 3, 5 are not represented */
    cnt -= 1;   /* 1 is represented and is not prime */

    /* tail: 30*QMAX .. N-1 (fewer than 30 numbers) */
    for (long long v = 30LL * QMAX; v < N; v++)
        if (is_prime_small(v, primes, np)) cnt++;

    printf("%lld\n", cnt);
    free(bm);
    free(primes);
    return 0;
}
