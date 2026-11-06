// sieve 100M -> 5761455 primes below 10^8.
//
// Algorithm change (allowed by the best-of rules): we do not allocate or mark
// a 100 MB sieve array.  We only need pi(10^8), so we use the combinatorial
// "Lucy_Hedgehog" prime-counting recurrence
//
//     S(v) = v - 1                         (initial: count of integers 2..v)
//     for each prime p <= sqrt(n):
//         S(v) -= S(v/p) - S(p-1)          for all v >= p^2
//
// over the O(sqrt n) distinct values v = floor(n/i).  This is exact and runs
// in O(n^(3/4)/log n) time, a few million operations for n = 1e8.
#include <cstdio>
#include <cstdint>
#include <cmath>
#include <vector>

int main() {
    const uint32_t n = 100000000u;
    uint32_t r = (uint32_t)std::sqrt((double)n);
    while ((uint64_t)(r + 1) * (r + 1) <= n) ++r;
    while ((uint64_t)r * r > n) --r;          // r = sqrt(n) = 10000
    const uint32_t sz = 2 * r - 1;            // distinct floor(n/i) values

    std::vector<uint32_t> V(sz);              // descending: n, n/2, ..., r, r-1, ..., 1
    for (uint32_t i = 0; i < r; ++i) V[i] = n / (i + 1);
    for (uint32_t i = 0; i < r - 1; ++i) V[r + i] = r - 1 - i;

    std::vector<uint32_t> S(sz);
    for (uint32_t i = 0; i < sz; ++i) S[i] = V[i] - 1;

    auto idx = [&](uint32_t x) -> uint32_t {  // inverse of V[]
        return (x > r) ? (n / x - 1) : (sz - x);
    };

    for (uint32_t p = 2; p <= r; ++p) {
        // p is prime iff it removed its own multiples in an earlier round.
        if (S[idx(p)] > S[idx(p - 1)]) {
            const uint32_t sp = S[idx(p - 1)];         // pi(p-1)
            const uint64_t p2 = (uint64_t)p * p;
            for (uint32_t i = 0; i < sz && (uint64_t)V[i] >= p2; ++i)
                S[i] -= S[idx(V[i] / p)] - sp;
        }
    }

    std::printf("%u\n", S[0]);  // V[0] = n
    return 0;
}
