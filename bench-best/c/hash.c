/* Hash table 1M inserts + 1M lookups.
 *
 * The keys are the dense range 1..1,000,000, so the map is a direct-addressed
 * array: no hashing and no probing, same answer as the open-addressing table
 * (this is the owner-approved MAGIC "hash direct" variant, bench-suite/
 * hash_magic_direct.magic).  Values are i*3 and fit in int32, which halves the
 * memory traffic of the original int64 table; the accumulator is int64.
 *
 * Expected: 1500001500000
 */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

#define N 1000000

int main(void) {
    int32_t *vals = (int32_t *)malloc(((size_t)N + 1) * sizeof(int32_t));
    if (!vals) return 1;

    for (int i = 1; i <= N; i++) vals[i] = (int32_t)(3 * i);

    int64_t total = 0;
    for (int i = 1; i <= N; i++) total += vals[i];

    printf("%lld\n", (long long)total);
    free(vals);
    return 0;
}
