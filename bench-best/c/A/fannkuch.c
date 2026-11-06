/* Best-of fannkuch-redux, n = 10.  Expected output:
 *     73196
 *     38
 *
 * The checksum depends on the *global* enumeration index (even indices add
 * the flip count, odd indices subtract), so the reference permutation order
 * matters and the sign is not a function of inversion parity alone.  The
 * reference generator is the standard rotation odometer: every 8! = 40320
 * emissions its lower seven radix counters complete one full cycle, and at
 * those points the generator state has a closed form.  For chunk
 * t = 9*b + a (a = t mod 9, b = t div 9), the state at the chunk boundary is
 *
 *     perm1[i] = ((i + a) mod 9 + b) mod 10   (i < 9),  perm1[9] = (9+b) % 10
 *     count[1..7] = 2..8,   count[8] = 9-a,   count[9] = 10-b,   r = 1
 *
 * (verified against a full serial run of the reference generator for all 90
 * chunk boundaries).  That makes the chunks completely independent: each of
 * the 90 OpenMP tasks starts from its closed-form state, replays exactly the
 * reference rotation sequence, and accumulates +/-flips with the true global
 * index parity.  The result is bit-identical to the reference algorithm.
 *
 * The flip loop is a switch with fully unrolled reversals for k = 1..9.
 */
#include <stdio.h>
#include <string.h>
#include <omp.h>

#define N 10
#define CHUNK 40320            /* 8! */
#define NTASK 90               /* 10! / 8! */
#ifndef NTHREADS
#define NTHREADS 12
#endif

static inline int flip_count(const unsigned char *restrict src)
{
    unsigned char p[N];
    memcpy(p, src, (size_t)N);

    int flips = 0;
    int k = p[0];
    while (k) {
        unsigned char t;
        switch (k) {
        case 1:
            t = p[0]; p[0] = p[1]; p[1] = t;
            break;
        case 2:
            t = p[0]; p[0] = p[2]; p[2] = t;
            break;
        case 3:
            t = p[0]; p[0] = p[3]; p[3] = t;
            t = p[1]; p[1] = p[2]; p[2] = t;
            break;
        case 4:
            t = p[0]; p[0] = p[4]; p[4] = t;
            t = p[1]; p[1] = p[3]; p[3] = t;
            break;
        case 5:
            t = p[0]; p[0] = p[5]; p[5] = t;
            t = p[1]; p[1] = p[4]; p[4] = t;
            t = p[2]; p[2] = p[3]; p[3] = t;
            break;
        case 6:
            t = p[0]; p[0] = p[6]; p[6] = t;
            t = p[1]; p[1] = p[5]; p[5] = t;
            t = p[2]; p[2] = p[4]; p[4] = t;
            break;
        case 7:
            t = p[0]; p[0] = p[7]; p[7] = t;
            t = p[1]; p[1] = p[6]; p[6] = t;
            t = p[2]; p[2] = p[5]; p[5] = t;
            t = p[3]; p[3] = p[4]; p[4] = t;
            break;
        case 8:
            t = p[0]; p[0] = p[8]; p[8] = t;
            t = p[1]; p[1] = p[7]; p[7] = t;
            t = p[2]; p[2] = p[6]; p[6] = t;
            t = p[3]; p[3] = p[5]; p[5] = t;
            break;
        default: /* k == 9 */
            t = p[0]; p[0] = p[9]; p[9] = t;
            t = p[1]; p[1] = p[8]; p[8] = t;
            t = p[2]; p[2] = p[7]; p[7] = t;
            t = p[3]; p[3] = p[6]; p[6] = t;
            t = p[4]; p[4] = p[5]; p[5] = t;
            break;
        }
        flips++;
        k = p[0];
    }
    return flips;
}

/* Advance the reference generator by exactly one permutation.  Returns 0 when
 * the whole 10! space has been exhausted. */
static inline int advance(unsigned char *restrict perm1, int *restrict count,
                          int *restrict r)
{
    for (;;) {
        if (*r == N) return 0;
        const unsigned char p0 = perm1[0];
        for (int i = 0; i < *r; i++) perm1[i] = perm1[i + 1];
        perm1[*r] = p0;
        if (--count[*r] > 0) return 1;
        (*r)++;
    }
}

int main(void)
{
    long long checksum = 0;
    int max_flips = 0;

#pragma omp parallel for schedule(dynamic, 1) num_threads(NTHREADS) \
    reduction(+:checksum) reduction(max:max_flips)
    for (int t = 0; t < NTASK; t++) {
        const int b = t / 9;
        const int a = t % 9;

        unsigned char perm1[N];
        int count[N];
        for (int i = 0; i < 9; i++)
            perm1[i] = (unsigned char)(((i + a) % 9 + b) % 10);
        perm1[9] = (unsigned char)((9 + b) % 10);
        memset(count, 0, sizeof count);
        for (int i = 1; i <= 7; i++) count[i] = i + 1;
        count[8] = 9 - a;
        count[9] = 10 - b;
        int r = 1;

        long long sum = 0;
        int mx = 0;
        const long long base = (long long)t * CHUNK;

        for (int j = 0; j < CHUNK; j++) {
            while (r != 1) { count[r - 1] = r; r--; }

            const int f = flip_count(perm1);
            sum += ((base + j) & 1) ? -(long long)f : (long long)f;
            if (f > mx) mx = f;

            if (j + 1 < CHUNK) advance(perm1, count, &r);
        }

        /* reduction into the shared checksum / max */
        checksum += sum;
        if (mx > max_flips) max_flips = mx;
    }

    printf("%lld\n%d\n", checksum, max_flips);
    return 0;
}
