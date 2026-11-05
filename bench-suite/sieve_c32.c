/* Sieve, identical work, 32-bit inner index. Expected: 664579. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#define N 100000000

int main(void) {
    unsigned char *s = (unsigned char *)malloc(N);
    if (!s) return 1;
    memset(s, 1, N);

    for (int i = 2; (long long)i * i < N; i++) {
        if (s[i]) {
            for (int j = i * i; j < N; j += i) s[j] = 0;
        }
    }

    long long count = 0;
    for (int i = 2; i < N; i++) {
        if (s[i]) count++;
    }
    printf("%lld\n", count);
    free(s);
    return 0;
}
