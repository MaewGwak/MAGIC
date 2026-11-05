/* mini-PostgreSQL: tokenizer + parser + executor, mirroring sqldb_magic.magic. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>
#include <string.h>

static bool is_digit(uint8_t c) { return c >= 48 && c <= 57; }
static bool is_alpha(uint8_t c) {
    return (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c == 95;
}
static uint8_t upper(uint8_t c) { return (c >= 97 && c <= 122) ? (uint8_t)(c - 32) : c; }

typedef struct { int32_t kind, start, len; int64_t val; } Tok;

static bool eq_word(const uint8_t *buf, const Tok *t, const char *s) {
    int32_t n = t->len;
    if (n != (int32_t)strlen(s)) return false;
    for (int32_t i = 0; i < n; i++) {
        if (upper(buf[t->start + i]) != upper((uint8_t)s[i])) return false;
    }
    return true;
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
    const size_t CAP = 8388608;
    uint8_t *buf = malloc(CAP);
    FILE *f = fopen("bench-db/sqldb.dat", "rb");
    if (!buf || !f) return 1;
    size_t n = fread(buf, 1, CAP, f);
    bcnt = calloc(BCAP, sizeof(int32_t));
    bch = calloc((size_t)BCAP * CS, sizeof(int32_t));
    bnext = calloc(BCAP, sizeof(int32_t));
    bleaf = calloc(BCAP, sizeof(uint8_t));
    bkeys = calloc((size_t)BCAP * KS, sizeof(int64_t));
    bvals = calloc((size_t)BCAP * KS, sizeof(int64_t));
    bleaf[0] = 1; bnext[0] = -1;
    fclose(f);

    Tok *tok = malloc(2000000 * sizeof(Tok));
    int32_t nt = 0;
    size_t i = 0;
    while (i < n) {
        uint8_t c = buf[i];
        if (c == 32 || c == 10 || c == 13 || c == 9) {
            i++;
        } else if (is_digit(c)) {
            int64_t v = 0;
            size_t s0 = i;
            while (i < n && is_digit(buf[i])) { v = v * 10 + (int64_t)(buf[i] - 48); i++; }
            tok[nt++] = (Tok){1, (int32_t)s0, (int32_t)(i - s0), v};
        } else if (is_alpha(c)) {
            size_t s0 = i;
            while (i < n && (is_alpha(buf[i]) || is_digit(buf[i]))) i++;
            tok[nt++] = (Tok){0, (int32_t)s0, (int32_t)(i - s0), 0};
        } else {
            tok[nt++] = (Tok){2, (int32_t)i, 1, 0};
            i++;
        }
    }

    int64_t *rk = malloc(200000 * sizeof(int64_t));
    int64_t *rv = malloc(200000 * sizeof(int64_t));
    int64_t rows = 0, checksum = 0;
    int32_t p = 0;
    while (p < nt) {
        if (tok[p].kind == 0 && eq_word(buf, &tok[p], "INSERT")) {
            rk[rows] = tok[p + 5].val;
            rv[rows] = tok[p + 7].val;
            btree_insert(rk[rows], rows);
            rows++;
            p += 9;
        } else if (tok[p].kind == 0 && eq_word(buf, &tok[p], "SELECT")) {
            int32_t q = p + 1;
            if (tok[q].kind == 0 && eq_word(buf, &tok[q], "COUNT")) {
                checksum += rows;
                p = q + 6;
            } else if (tok[q].kind == 0 && eq_word(buf, &tok[q], "SUM")) {
                int64_t total = 0;
                for (int64_t r = 0; r < rows; r++) total += rv[r];
                checksum += total;
                p = q + 6;
            } else {
                int64_t key = tok[p + 7].val;
                int64_t found = 0;
                if (argc > 1 && strcmp(argv[1], "index") == 0) {
                    int64_t ri = btree_search(key);
                    if (ri >= 0) found = rv[ri];
                } else {
                    for (int64_t r = 0; r < rows; r++) if (rk[r] == key) found = rv[r];
                }
                checksum += found;
                p += 9;
            }
        } else {
            p++;
        }
    }
    printf("%lld\n%lld\n", (long long)rows, (long long)checksum);
    return 0;
}
