/* Open-addressing hash table: 1M inserts + 1M lookups. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#define SIZE 2097152
#define N 1000000

int main(void) {
    int64_t *keys = (int64_t *)calloc(SIZE, sizeof(int64_t));
    int64_t *vals = (int64_t *)calloc(SIZE, sizeof(int64_t));
    if (!keys || !vals) return 1;

    for (int64_t i = 1; i <= N; i++) {
        int64_t slot = (i * 2654435761LL) % SIZE;
        while (keys[slot] != 0) slot = (slot + 1) % SIZE;
        keys[slot] = i;
        vals[slot] = i * 3;
    }

    int64_t total = 0;
    for (int64_t i = 1; i <= N; i++) {
        int64_t slot = (i * 2654435761LL) % SIZE;
        while (keys[slot] != i) slot = (slot + 1) % SIZE;
        total += vals[slot];
    }
    printf("%lld\n", (long long)total);
    free(keys);
    free(vals);
    return 0;
}
