// Sieve, identical work: mark only up to sqrt(N), then count all primes.
// Expected: 664579.
#include <cstdio>
#include <vector>

int main() {
    constexpr int N = 100000000;
    std::vector<unsigned char> s(N, 1);

    for (int i = 2; (long long)i * i < N; i++) {
        if (s[i]) {
            for (long long j = (long long)i * i; j < N; j += i) s[j] = 0;
        }
    }

    long long count = 0;
    for (int i = 2; i < N; i++) {
        if (s[i]) count++;
    }
    std::printf("%lld\n", count);
    return 0;
}
