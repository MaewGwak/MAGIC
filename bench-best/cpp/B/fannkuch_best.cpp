// fannkuch n=10 -> checksum 73196, max flips 38.
//
// Fastest reasonable version.  Two things make it much faster than the
// reference rotation machine:
//
// 1. Permutations are enumerated in lexicographic order, split two-deep
//    (90 independent jobs of 8! permutations), so the work parallelises
//    perfectly across OpenMP threads.
//
// 2. The checksum sign is the parity of the permutation's *position* in the
//    reference generator's order.  That generator is a nested rotation
//    machine: at level m it emits m blocks of (m-1)! permutations and rotates
//    the prefix; because (m-1)! is even for m >= 3, the parity is decided by
//    the innermost 2-element block, which reduces to a closed form on the
//    first three elements.  Writing a=perm[0], b=perm[1], c=perm[2] and
//    "successor" = next value in the cyclic ascending order of {a,b,c}:
//        generator index is odd  <=>  successor(c) != a.
//    (Verified against the reference generator for n = 3..7.)  So we can
//    enumerate lexicographically and still reproduce the exact checksum.
//
// 3. Flip counting works on a packed 4-bit-per-element representation: a
//    prefix reversal is a nibble reversal, done with bswap + nibble swap
//    instead of an element-by-element swap loop.
#include <cstdio>
#include <cstdint>
#include <cstring>

#if defined(__BMI2__)
#include <immintrin.h>
#endif

namespace {

inline int parity_bit(int a, int b, int c) {
    // 1 iff the reference generator's index for this permutation is odd.
    // In the cyclic ascending order of the trio, is a the successor of c?
    int nxt;
    if (a > c) nxt = (b > c && b < a) ? b : a;
    else       nxt = (b > c) ? b : ((a < b) ? a : b);
    return (nxt == a) ? 0 : 1;
}

// Specialised lexicographic successor for the 8 suffix elements: the same
// algorithm as std::next_permutation, but with no generic-iterator machinery
// (and no GCC 16 -Wstringop-overflow false positive on the 8-byte array).
inline bool next_perm(uint8_t* a) {
    int i = 6;
    while (i >= 0 && a[i] >= a[i + 1]) --i;
    if (i < 0) return false;
    int j = 7;
    while (a[j] <= a[i]) --j;
    uint8_t t = a[i]; a[i] = a[j]; a[j] = t;
    for (int l = i + 1, r = 7; l < r; ++l, --r) { t = a[l]; a[l] = a[r]; a[r] = t; }
    return true;
}

inline uint64_t pack8(const uint8_t* r) {
#if defined(__BMI2__)
    uint64_t w;
    std::memcpy(&w, r, 8);
    return _pext_u64(w, 0x0F0F0F0F0F0F0F0FULL);  // 8 nibbles, r[0] lowest
#else
    uint64_t p = 0;
    for (int i = 7; i >= 0; --i) p = (p << 4) | r[i];
    return p;
#endif
}

inline int flip_count(uint64_t p) {
    constexpr uint64_t LO = 0x0F0F0F0F0F0F0F0FULL;
    constexpr uint64_t HI = 0xF0F0F0F0F0F0F0F0ULL;
    int f = 0;
    while (p & 0xFULL) {
        const unsigned k = (unsigned)(p & 0xFULL);  // 1..9
        const unsigned n = k + 1;                   // reversed prefix length
        const uint64_t mask = (n == 10) ? 0xFFFFFFFFFFULL
                                        : ((1ULL << (4 * n)) - 1ULL);
        const unsigned sh = 64 - 4 * n;
        // Move the prefix to the top nibbles, then reverse all 16 nibbles:
        // bswap reverses the byte order, the nibble swap reverses inside
        // bytes; together they put the reversed prefix back in the low
        // n nibbles (the vacated high nibbles are zero).
        uint64_t t = __builtin_bswap64((p & mask) << sh);
        t = ((t & LO) << 4) | ((t & HI) >> 4);
        p = (p & ~mask) | t;
        ++f;
    }
    return f;
}

}  // namespace

int main() {
    constexpr int N = 10;
    constexpr int NJOBS = N * (N - 1);  // fix first two elements
    long long checksum = 0;
    int maxflips = 0;

#pragma omp parallel for schedule(dynamic, 1) reduction(+ : checksum) reduction(max : maxflips)
    for (int t = 0; t < NJOBS; ++t) {
        const int i0 = t / (N - 1);
        const int i1raw = t % (N - 1);

        int all[N];
        for (int v = 0; v < N; ++v) all[v] = v;
        const int i1 = all[i1raw + (i1raw >= i0)];

        uint8_t rest[N - 2];
        int m = 0;
        for (int v = 0; v < N; ++v)
            if (v != i0 && v != i1) rest[m++] = (uint8_t)v;

        const uint64_t base = (uint64_t)i0 | ((uint64_t)i1 << 4);
        do {
            const uint64_t packed = base | (pack8(rest) << 8);
            const int f = flip_count(packed);
            checksum += parity_bit(i0, i1, rest[0]) ? -f : f;
            if (f > maxflips) maxflips = f;
        } while (next_perm(rest));
    }

    std::printf("%lld\n%d\n", checksum, maxflips);
    return 0;
}
