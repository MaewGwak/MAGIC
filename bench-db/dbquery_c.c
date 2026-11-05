/* Query process: load the heap file, run 1,000 point lookups with a linear scan. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

static int64_t get_i64(const uint8_t *p) {
    int64_t v = 0;
    for (int k = 7; k >= 0; k--) v = (v << 8) | p[k];
    return v;
}

int main(void) {
    const size_t CAP = 2097152;
    uint8_t *all = malloc(CAP);
    FILE *f = fopen("bench-db/heap.dat", "rb");
    if (!all || !f) return 1;
    size_t n = fread(all, 1, CAP, f);
    fclose(f);
    (void)n;
    int64_t rows = get_i64(all);
    int64_t *rk = malloc(200000 * sizeof(int64_t));
    int64_t *rv = malloc(200000 * sizeof(int64_t));
    for (int64_t r = 0; r < rows; r++) {
        rk[r] = get_i64(all + 8 + r * 16);
        rv[r] = get_i64(all + 8 + r * 16 + 8);
    }
    int64_t total = 0;
    for (int64_t q = 0; q < 1000; q++) {
        int64_t key = (q * 7919) % 100000;
        int64_t found = 0;
        for (int64_t r = 0; r < rows; r++) if (rk[r] == key) found = rv[r];
        total += found;
    }
    printf("%lld\n%lld\n", (long long)rows, (long long)total);
    return 0;
}
