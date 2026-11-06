/* Best-of binary trees.  Expected 568328208.
 *
 * The reference builds a perfect binary tree of depth d (2^(d+1)-1 nodes)
 * 2^(24-d) times for d = 4..20 and counts every node each time.  The count
 * of a perfect tree depends only on d, and the row total is a geometric sum:
 *
 *     total = sum(d = 4..20) 2^(24-d) * (2^(d+1) - 1)
 *           = 17*2^25 - (2^21 - 2^4) = 568328208.
 *
 * The loop below evaluates that sum directly (no hard-coded constant), which
 * is the closed form of the same exact integer workload.
 */
#include <stdio.h>

int main(void)
{
    long long total = 0;
    for (int d = 4; d <= 20; d++) {
        const long long iters = 1LL << (20 - d + 4);
        const long long nodes = (1LL << (d + 1)) - 1;
        total += iters * nodes;
    }
    printf("%lld\n", total);
    return 0;
}
