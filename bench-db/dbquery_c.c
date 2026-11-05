/* Query process: load the heap file, run 1,000 point lookups with a linear scan. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>

static int64_t get_i64(const uint8_t *p) {
    int64_t v = 0;
    for (int k = 7; k >= 0; k--) v = (v << 8) | p[k];
    return v;
}

#define KMAX 64
#define KS (KMAX + 1)
#define CS (KMAX + 2)
#define BCAP 20000

static int32_t bnodes = 1, broot = 0;
static int32_t *bcnt, *bch, *bnext;
static uint8_t *bleaf;
static int64_t *bkeys, *bvals;

static int32_t bb(int32_t node, int64_t key, bool gt) {
    int32_t lo = 0, hi = bcnt[node];
    while (lo < hi) {
        int32_t mid = (lo + hi) / 2;
        int64_t k = bkeys[(int64_t)node * KS + mid];
        if (gt ? (k <= key) : (k < key)) lo = mid + 1;
        else hi = mid;
    }
    return lo;
}

static void btree_insert(int64_t key, int64_t val) {
    int32_t path_node[32], path_idx[32];
    int32_t node = broot, depth = 0;
    while (!bleaf[node]) {
        int32_t idx = bb(node, key, true);
        path_node[depth] = node; path_idx[depth] = idx; depth++;
        node = bch[(int64_t)node * CS + idx];
    }
    int32_t pos = bb(node, key, false);
    for (int32_t j = bcnt[node]; j > pos; j--) {
        bkeys[(int64_t)node * KS + j] = bkeys[(int64_t)node * KS + j - 1];
        bvals[(int64_t)node * KS + j] = bvals[(int64_t)node * KS + j - 1];
    }
    bkeys[(int64_t)node * KS + pos] = key;
    bvals[(int64_t)node * KS + pos] = val;
    bcnt[node]++;

    int64_t sep = 0; int32_t new_child = -1; bool split = false;
    if (bcnt[node] > KMAX) {
        int32_t nn = bnodes++; bleaf[nn] = 1;
        int32_t mid = KMAX / 2, m = mid, t = 0;
        while (m < bcnt[node]) {
            bkeys[(int64_t)nn * KS + t] = bkeys[(int64_t)node * KS + m];
            bvals[(int64_t)nn * KS + t] = bvals[(int64_t)node * KS + m];
            t++; m++;
        }
        bcnt[nn] = t; bcnt[node] = mid;
        bnext[nn] = bnext[node]; bnext[node] = nn;
        sep = bkeys[(int64_t)nn * KS]; new_child = nn; split = true;
    }
    while (split && depth > 0) {
        depth--;
        int32_t pnode = path_node[depth], pidx = path_idx[depth];
        for (int32_t j2 = bcnt[pnode]; j2 > pidx; j2--) {
            bkeys[(int64_t)pnode * KS + j2] = bkeys[(int64_t)pnode * KS + j2 - 1];
        }
        bkeys[(int64_t)pnode * KS + pidx] = sep;
        for (int32_t c2 = bcnt[pnode] + 1; c2 > pidx + 1; c2--) {
            bch[(int64_t)pnode * CS + c2] = bch[(int64_t)pnode * CS + c2 - 1];
        }
        bch[(int64_t)pnode * CS + pidx + 1] = new_child;
        bcnt[pnode]++;
        if (bcnt[pnode] <= KMAX) { split = false; }
        else {
            int32_t nn2 = bnodes++; bleaf[nn2] = 0;
            int32_t mid2 = KMAX / 2, mm = mid2 + 1, tt = 0;
            int64_t med = bkeys[(int64_t)pnode * KS + mid2];
            while (mm < bcnt[pnode]) {
                bkeys[(int64_t)nn2 * KS + tt] = bkeys[(int64_t)pnode * KS + mm];
                tt++; mm++;
            }
            int32_t cc = mid2 + 1, tc = 0;
            while (cc <= bcnt[pnode]) {
                bch[(int64_t)nn2 * CS + tc] = bch[(int64_t)pnode * CS + cc];
                tc++; cc++;
            }
            bcnt[nn2] = tt; bcnt[pnode] = mid2;
            sep = med; new_child = nn2;
        }
    }
    if (split) {
        int32_t nr = bnodes++; bleaf[nr] = 0;
        bkeys[(int64_t)nr * KS] = sep;
        bch[(int64_t)nr * CS] = broot;
        bch[(int64_t)nr * CS + 1] = new_child;
        bcnt[nr] = 1;
        broot = nr;
    }
}

static int64_t btree_search(int64_t key) {
    int32_t node = broot;
    while (!bleaf[node]) node = bch[(int64_t)node * CS + bb(node, key, true)];
    int32_t pos = bb(node, key, false);
    if (pos < bcnt[node] && bkeys[(int64_t)node * KS + pos] == key) {
        return bvals[(int64_t)node * KS + pos];
    }
    return -1;
}

int main(int argc, char **argv) {
    const size_t CAP = 2097152;
    uint8_t *all = malloc(CAP);
    FILE *f = fopen("bench-db/heap.dat", "rb");
    if (!all || !f) return 1;
    size_t n = fread(all, 1, CAP, f);
    fclose(f);
    (void)n;
    int64_t rows = get_i64(all);
    bcnt = calloc(BCAP, sizeof(int32_t));
    bch = calloc((size_t)BCAP * CS, sizeof(int32_t));
    bnext = calloc(BCAP, sizeof(int32_t));
    bleaf = calloc(BCAP, sizeof(uint8_t));
    bkeys = calloc((size_t)BCAP * KS, sizeof(int64_t));
    bvals = calloc((size_t)BCAP * KS, sizeof(int64_t));
    bleaf[0] = 1; bnext[0] = -1;
    int64_t *rk = malloc(200000 * sizeof(int64_t));
    int64_t *rv = malloc(200000 * sizeof(int64_t));
    for (int64_t r = 0; r < rows; r++) {
        rk[r] = get_i64(all + 8 + r * 16);
        rv[r] = get_i64(all + 8 + r * 16 + 8);
    }
    for (int64_t r = 0; r < rows; r++) btree_insert(rk[r], r);
    int indexed = (argc > 1 && strcmp(argv[1], "index") == 0);
    int64_t total = 0;
    for (int64_t q = 0; q < 10000; q++) {
        int64_t key = (q * 7919) % 100000;
        int64_t found = 0;
        if (indexed) {
            int64_t ri = btree_search(key);
            if (ri >= 0) found = rv[ri];
        } else {
            for (int64_t r = 0; r < rows; r++) if (rk[r] == key) found = rv[r];
        }
        total += found;
    }
    printf("%lld\n%lld\n", (long long)rows, (long long)total);
    return 0;
}
