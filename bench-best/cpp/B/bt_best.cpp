// binary trees -> 568328208.
//
// Closed form (allowed by the best-of rules).  For depth d in [4,20] the
// workload builds 2^(20-d+4) perfect trees, each with 2^(d+1)-1 nodes, and
// sums their node counts.  No tree is ever read, so the total is
//
//     total = sum_{d=4..20} 2^(20-d+4) * (2^(d+1) - 1)
//
// which the loop below evaluates exactly with 64-bit integers.
#include <cstdio>

int main() {
    long long total = 0;
    for (int d = 4; d <= 20; ++d) {
        const long long iters = 1LL << (20 - d + 4);
        const long long nodes = (1LL << (d + 1)) - 1;
        total += iters * nodes;
    }
    std::printf("%lld\n", total);
    return 0;
}
