// Best-of C++: fannkuch-redux, n = 10.
//
// Workload (bench-fannkuch/fannkuch_c.c): generate all 10! permutations in the
// reference "rotating prefix" order, count the pancake flips of each, and print
//   checksum = sum over the enumeration of (+flips, -flips, +flips, ...)
//   max_flips
//
// Parallel decomposition (validated against the sequential reference for
// n = 8, 9, 10): the reference odometer is a nested counter.  A block of m!
// consecutive permutations is obtained by starting from the identity and
// left-rotating the prefix of length L by b_L for L = n down to m+1; inside a
// block the same odometer runs over the first m positions.  Every block holds
// m! permutations and m! is even, so every block starts at an even global index
// and the local +,-,+,- pattern is the global one.  With n=10, m=7 that is
// 10*9*8 = 720 independent blocks of 5040 permutations, one OpenMP task each.
//
// The permutation lives in one 128-bit register, one byte per element.  A flip
// (reverse the first k+1 elements) is a single PSHUFB with a mask selected by
// k, and the odometer step (rotate the first r+1 elements left by one) is a
// single PSHUFB as well; there is no per-permutation copy and no load/store
// reversal loop.  This roughly halves single-thread time versus the textbook
// array implementation (165 ms -> ~88 ms here).  Requires SSSE3, which
// -march=native provides on every x86-64 v2+ host.
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cstdint>
#include <algorithm>
#include <tmmintrin.h>
#include <omp.h>

namespace {

typedef long long ll;

constexpr int N = 10;
constexpr int M = 7;                 // permutations per block = M!
constexpr int NBLOCKS = N * (N - 1) * (N - 2);   // 720

__m128i revmask[12];   // revmask[m]: reverse the first m bytes, keep the rest
__m128i rotmask[12];   // rotmask[l]: rotate the first l bytes left by one

void init_masks() {
    for (int m = 1; m <= 10; m++) {
        unsigned char idx[16];
        for (int i = 0; i < 16; i++) idx[i] = (unsigned char)((i < m) ? (m - 1 - i) : i);
        revmask[m] = _mm_loadu_si128((const __m128i *)idx);
    }
    for (int l = 2; l <= 11; l++) {
        unsigned char idx[16];
        for (int i = 0; i < 16; i++)
            idx[i] = (unsigned char)((i < l - 1) ? (i + 1) : ((i == l - 1) ? 0 : i));
        rotmask[l] = _mm_loadu_si128((const __m128i *)idx);
    }
}

// pancake-flip count of a permutation packed one byte per position
inline int flips_of(__m128i x) {
    int f = 0;
    for (;;) {
        const int k = _mm_cvtsi128_si32(x) & 0xFF;   // perm[0]
        if (k == 0) break;
        x = _mm_shuffle_epi8(x, revmask[k + 1]);     // reverse perm[0..k]
        f++;
    }
    return f;
}

inline void rotate_prefix(int *a, int len, int by) {
    int t[16];
    for (int i = 0; i < len; i++) t[i] = a[i];
    for (int i = 0; i < len; i++) {
        int s = i + by;
        if (s >= len) s -= len;
        a[i] = t[s];
    }
}

inline void run_block(const int *base, int *count, int &mx, ll &sum) {
    unsigned char buf[16] = {0};
    for (int i = 0; i < N; i++) buf[i] = (unsigned char)base[i];
    __m128i x = _mm_loadu_si128((const __m128i *)buf);
    int r = M;
    int parity = 0;
    for (;;) {
        while (r != 1) {
            count[r - 1] = r;
            r--;
        }
        const int fl = flips_of(x);
        if (fl > mx) mx = fl;
        sum += parity ? -fl : fl;
        parity ^= 1;
        for (;;) {
            if (r == M) return;
            x = _mm_shuffle_epi8(x, rotmask[r + 1]);
            if (--count[r] > 0) break;
            r++;
        }
    }
}

}  // namespace

// This kernel is branch/ALU bound and SMT siblings share the same execution
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
    init_masks();
    const int nthreads = physical_cores();

    ll checksum = 0;
    int maxflips = 0;

#pragma omp parallel num_threads(nthreads)
    {
        int count[16];
        int mymx = 0;
        ll mysum = 0;
#pragma omp for schedule(dynamic, 2)
        for (int b = 0; b < NBLOCKS; b++) {
            int base[N];
            for (int i = 0; i < N; i++) base[i] = i;
            int v = b;
            rotate_prefix(base, N, v % N);                       // level 10
            rotate_prefix(base, N - 1, (v / N) % (N - 1));       // level 9
            rotate_prefix(base, N - 2, (v / (N * (N - 1))));     // level 8
            int lmx = 0;
            ll lsum = 0;
            run_block(base, count, lmx, lsum);
            mymx = std::max(mymx, lmx);
            mysum += lsum;
        }
#pragma omp critical
        {
            checksum += mysum;
            if (mymx > maxflips) maxflips = mymx;
        }
    }

    std::printf("%lld\n%d\n", checksum, maxflips);
    return 0;
}
