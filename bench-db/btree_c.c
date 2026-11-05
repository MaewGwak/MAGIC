/* In-memory B+tree over i64 keys, MAXK keys per node. Same algorithm as
   btree_magic.magic: N unique keys inserted, then looked up and checksummed. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

#define MAXK 64
#define CAP 20000
#define N 200000

static int32_t bisect(int32_t node, int64_t key, const int32_t *cnt, const int64_t *keys) {
    int32_t lo = 0, hi = cnt[node];
    while (lo < hi) {
        int32_t mid = (lo + hi) / 2;
        if (keys[(int64_t)node * MAXK + mid] < key) lo = mid + 1;
        else hi = mid;
    }
    return lo;
}

int main(void) {
    int32_t *cnt = calloc(CAP, sizeof(int32_t));
    uint8_t *leaf = calloc(CAP, sizeof(uint8_t));
    int32_t *next = calloc(CAP, sizeof(int32_t));
    int64_t *keys = calloc((size_t)CAP * MAXK, sizeof(int64_t));
    int64_t *vals = calloc((size_t)CAP * MAXK, sizeof(int64_t));
    int32_t *ch = calloc((size_t)CAP * (MAXK + 1), sizeof(int32_t));
    int32_t path_node[32], path_idx[32];
    if (!cnt || !leaf || !next || !keys || !vals || !ch) return 1;

    int32_t nodes = 1, root = 0;
    leaf[0] = 1;
    next[0] = -1;

    for (int64_t i = 0; i < N; i++) {
        int64_t key = (i * 2654435761LL) % 1000000007LL;
        int64_t val = key * 3;

        int32_t node = root, depth = 0;
        while (!leaf[node]) {
            int32_t idx = bisect(node, key, cnt, keys);
            path_node[depth] = node;
            path_idx[depth] = idx;
            depth++;
            node = ch[(int64_t)node * (MAXK + 1) + idx];
        }

        int32_t pos = bisect(node, key, cnt, keys);
        bool dup = pos < cnt[node] && keys[(int64_t)node * MAXK + pos] == key;
        if (!dup) {
            for (int32_t j = cnt[node]; j > pos; j--) {
                keys[(int64_t)node * MAXK + j] = keys[(int64_t)node * MAXK + j - 1];
                vals[(int64_t)node * MAXK + j] = vals[(int64_t)node * MAXK + j - 1];
            }
            keys[(int64_t)node * MAXK + pos] = key;
            vals[(int64_t)node * MAXK + pos] = val;
            cnt[node]++;

            int64_t sep = 0;
            int32_t new_child = -1;
            bool split = false;
            if (cnt[node] > MAXK) {
                int32_t nn = nodes++;
                leaf[nn] = 1;
                int32_t mid = MAXK / 2, m = mid, t = 0;
                while (m < cnt[node]) {
                    keys[(int64_t)nn * MAXK + t] = keys[(int64_t)node * MAXK + m];
                    vals[(int64_t)nn * MAXK + t] = vals[(int64_t)node * MAXK + m];
                    t++; m++;
                }
                cnt[nn] = t;
                cnt[node] = mid;
                next[nn] = next[node];
                next[node] = nn;
                sep = keys[(int64_t)nn * MAXK];
                new_child = nn;
                split = true;
            }

            while (split && depth > 0) {
                depth--;
                int32_t pnode = path_node[depth], pidx = path_idx[depth];
                for (int32_t j2 = cnt[pnode]; j2 > pidx; j2--) {
                    keys[(int64_t)pnode * MAXK + j2] = keys[(int64_t)pnode * MAXK + j2 - 1];
                }
                keys[(int64_t)pnode * MAXK + pidx] = sep;
                for (int32_t c2 = cnt[pnode] + 1; c2 > pidx + 1; c2--) {
                    ch[(int64_t)pnode * (MAXK + 1) + c2] = ch[(int64_t)pnode * (MAXK + 1) + c2 - 1];
                }
                ch[(int64_t)pnode * (MAXK + 1) + pidx + 1] = new_child;
                cnt[pnode]++;
                if (cnt[pnode] <= MAXK) {
                    split = false;
                } else {
                    int32_t nn2 = nodes++;
                    leaf[nn2] = 0;
                    int32_t mid2 = MAXK / 2, mm = mid2 + 1, tt = 0;
                    int64_t med = keys[(int64_t)pnode * MAXK + mid2];
                    while (mm < cnt[pnode]) {
                        keys[(int64_t)nn2 * MAXK + tt] = keys[(int64_t)pnode * MAXK + mm];
                        tt++; mm++;
                    }
                    int32_t cc = mid2 + 1, tc = 0;
                    while (cc <= cnt[pnode]) {
                        ch[(int64_t)nn2 * (MAXK + 1) + tc] = ch[(int64_t)pnode * (MAXK + 1) + cc];
                        tc++; cc++;
                    }
                    cnt[nn2] = tt;
                    cnt[pnode] = mid2;
                    sep = med;
                    new_child = nn2;
                }
            }

            if (split) {
                int32_t nr = nodes++;
                leaf[nr] = 0;
                keys[(int64_t)nr * MAXK] = sep;
                ch[(int64_t)nr * (MAXK + 1)] = root;
                ch[(int64_t)nr * (MAXK + 1) + 1] = new_child;
                cnt[nr] = 1;
                root = nr;
            }
        }
    }

    int64_t total = 0;
    for (int64_t i = 0; i < N; i++) {
        int64_t key = (i * 2654435761LL) % 1000000007LL;
        int32_t node = root;
        while (!leaf[node]) {
            int32_t idx = bisect(node, key, cnt, keys);
            node = ch[(int64_t)node * (MAXK + 1) + idx];
        }
        int32_t pos = bisect(node, key, cnt, keys);
        if (pos < cnt[node] && keys[(int64_t)node * MAXK + pos] == key) {
            total += vals[(int64_t)node * MAXK + pos];
        }
    }
    printf("%lld\n%d\n", (long long)total, nodes);
    return 0;
}
