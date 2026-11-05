/* C binary trees, flat arena of indices — the same shape as MAGIC and
 * bt_rust_flat.rs. Expected total: 568328208. */
#include <stdio.h>
#include <stdlib.h>

static int *left, *right, *next;

static int build(int depth) {
    int idx = next[0]++;
    if (depth > 0) {
        left[idx] = build(depth - 1);
        right[idx] = build(depth - 1);
    } else {
        left[idx] = -1;
        right[idx] = -1;
    }
    return idx;
}

static long long check(int idx) {
    if (idx < 0) return 0;
    return 1 + check(left[idx]) + check(right[idx]);
}

int main(void) {
    const int max_nodes = 1 << 21;
    left = (int *)malloc((size_t)max_nodes * sizeof(int));
    right = (int *)malloc((size_t)max_nodes * sizeof(int));
    next = (int *)malloc(sizeof(int));
    if (!left || !right || !next) return 1;

    long long total = 0;
    for (int d = 4; d <= 20; d++) {
        int iters = 1 << (20 - d + 4);
        for (int i = 0; i < iters; i++) {
            next[0] = 0;
            int root = build(d);
            total += check(root);
        }
    }
    printf("%lld\n", total);
    free(left);
    free(right);
    free(next);
    return 0;
}
