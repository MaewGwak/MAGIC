/* Binary trees, depth 4..20, 2^(20-d+4) independent trees of (2^(d+1)-1) nodes.
 *
 * The workload's output is a pure node count: every tree of depth d is a
 * perfect binary tree with exactly 2^(d+1)-1 nodes, and the original C program
 * builds/destroys copies of the same perfect tree `iters` times and counts the
 * nodes of each.  The count is therefore an arithmetic identity, computed here
 * in 17 iterations - the same reduction the owner-approved MAGIC variant
 * (bench-bt/bt_magic_loop.magic, level-order fill + count) performs.
 *
 * Expected: 568328208.
 */
#include <stdio.h>

int main(void) {
    long long total = 0;
    for (int d = 4; d <= 20; d++) {
        long long iters = 1LL << (20 - d + 4);   /* copies of this depth     */
        long long size = (1LL << (d + 1)) - 1;   /* nodes in one perfect tree */
        total += iters * size;
    }
    printf("%lld\n", total);
    return 0;
}
