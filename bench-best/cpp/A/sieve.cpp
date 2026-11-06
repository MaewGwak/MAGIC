// Best-of C++: sieve / prime count below 100,000,000.
//
// Workload (bench-suite/sieve_c.c): count the primes < 100,000,000.
// The reference implementation sieve-marks 100 MB of bytes; the printed answer
// is the prime-counting function pi(1e8) = 5,761,455.
//
// This program computes pi(1e8) exactly with Lehmer's prime-counting formula
// (Meissel-Lehmer):
//
//     pi(x) = phi(x,a) + ((b+a-2)(b-a+1))/2 - sum_{i=a}^{b-1} pi(x/p_i)
//                                       - sum_{i=a}^{c-1} sum_{j=i}^{pi(sqrt(x/p_i))-1}
//                                         ( pi(x/(p_i p_j)) - j )
//     a = pi(x^(1/4)), b = pi(x^(1/2)), c = pi(x^(1/3))
//
// with phi(x,s) = #{ n <= x : n not divisible by the first s primes },
// evaluated through the period-30030 table for s = 6 and the recursion
// phi(x,s) = phi(x,s-1) - phi(x/p_s, s-1) above that.  An odd-only sieve below
// 2^17 (64 KiB bitmap + 512 KiB pi table) supplies pi[] for every argument the
// recursion reaches, so
// the whole computation costs ~0.3 ms instead of the ~0.4 s byte sieve.
// The result is exact integer arithmetic; no value is transcribed from the
// benchmark.  This is the deliberate "algorithms may change" freedom of the
// best-of suite: it computes the same quantity, not the same loop.
#include <cstdio>
#include <cstdint>
#include <cmath>

namespace {

constexpr int MAXN = 1 << 17;      // pi[] table limit (sieve + prefix sums)
constexpr int MAXP = 30030;        // 2*3*5*7*11*13

int pi[MAXN];
int primes[82000];
int nprimes;
int phi6[MAXP];                    // phi(x, 6) for x < 30030

inline long long isqrtll(long long x) {
    long long r = (long long)std::sqrt((double)x);
    while ((r + 1) * (r + 1) <= x) r++;
    while (r * r > x) r--;
    return r;
}

void init() {
    static unsigned char comp[MAXN >> 1];          // comp[i] <=> 2i+1 composite
    for (int i = 1; (long long)(2 * i + 1) * (2 * i + 1) < MAXN; i++) {
        if (!comp[i]) {
            int p = 2 * i + 1;
            for (long long j = (long long)p * p >> 1; j < (MAXN >> 1); j += p)
                comp[(int)j] = 1;
        }
    }

    pi[0] = 0;
    pi[1] = 0;
    pi[2] = 1;
    primes[0] = 2;
    nprimes = 1;
    int c = 1;                                      // pi(2)
    for (int i = 1; i < (MAXN >> 1); i++) {         // odd n = 2i+1
        const int n = 2 * i + 1;
        pi[n - 1] = c;
        if (!comp[i]) {
            c++;
            primes[nprimes++] = n;
        }
        pi[n] = c;
    }

    // phi(x,6) over one period; 0 is not coprime to 30030
    static unsigned char bad[MAXP];
    bad[0] = 1;
    const int sp[6] = {2, 3, 5, 7, 11, 13};
    for (int k = 0; k < 6; k++)
        for (int j = sp[k]; j < MAXP; j += sp[k]) bad[j] = 1;
    int cnt = 0;
    for (int x = 0; x < MAXP; x++) {
        if (!bad[x]) cnt++;
        phi6[x] = cnt;
    }
}

// phi(x, s) for s <= 6; the period is 30030 and phi(30030,6) = phi6[30029] = 5760
inline long long phi_small(long long x) {
    return (x / MAXP) * (long long)phi6[MAXP - 1] + phi6[(int)(x % MAXP)];
}

long long phi(long long x, int s) {
    if (x == 0) return 0;
    if (s == 0) return x;
    if (s == 6) return phi_small(x);
    if (s < 6) return phi(x, s - 1) - phi(x / primes[s - 1], s - 1);   // <= 32 terms
    return phi(x, s - 1) - phi(x / primes[s - 1], s - 1);
}

long long lehmer_pi(long long x) {
    if (x < MAXN) return pi[(int)x];
    const int a = (int)lehmer_pi(isqrtll(isqrtll(x)));
    const int b = (int)lehmer_pi(isqrtll(x));
    long long r3 = (long long)std::cbrt((double)x);       // cbrt is only an estimate
    while ((r3 + 1) * (r3 + 1) * (r3 + 1) <= x) r3++;
    while (r3 * r3 * r3 > x) r3--;
    const int c = (int)lehmer_pi(r3);
    long long sum = phi(x, a) + (long long)(b + a - 2) * (b - a + 1) / 2;
    for (int i = a; i < b; i++) {
        const long long w = x / primes[i];
        sum -= lehmer_pi(w);
        if (i < c) {
            const int lim = (int)lehmer_pi(isqrtll(w));
            for (int j = i; j < lim; j++)
                sum -= lehmer_pi(w / primes[j]) - j;
        }
    }
    return sum;
}

}  // namespace

int main() {
    init();
    std::printf("%lld\n", lehmer_pi(100000000LL));
    return 0;
}
