/* 1M inserts and 1M lookups in an open-addressing i64 -> i64 map. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

#define N 1000000
#define CAP 2097152

static void map_put(int64_t *keys, int64_t *vals, int64_t k, int64_t v) {
    int64_t h = (k * 2654435761LL) % CAP;
    while (keys[h] != 0) h = (h + 1) % CAP;
    keys[h] = k;
    vals[h] = v;
}

static int64_t map_get(const int64_t *keys, const int64_t *vals, int64_t k) {
    int64_t h = (k * 2654435761LL) % CAP;
    while (keys[h] != 0) {
        if (keys[h] == k) return vals[h];
        h = (h + 1) % CAP;
    }
    return 0;
}

int main(void) {
    int64_t *keys = calloc(CAP, sizeof(int64_t));
    int64_t *vals = calloc(CAP, sizeof(int64_t));
    if (!keys || !vals) return 1;
    for (int64_t i = 1; i <= N; i++) map_put(keys, vals, i, i * 3);
    int64_t total = 0;
    for (int64_t i = 1; i <= N; i++) total += map_get(keys, vals, i);
    printf("%lld\n", (long long)total);
    return 0;
}
