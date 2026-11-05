/* Integer matmul, N=384. Expected checksum printed below. */
#include <stdio.h>
#include <stdlib.h>
#define N 384

int main(void) {
    int *a = (int *)malloc((size_t)N * N * sizeof(int));
    int *b = (int *)malloc((size_t)N * N * sizeof(int));
    int *c = (int *)malloc((size_t)N * N * sizeof(int));
    if (!a || !b || !c) return 1;

    for (int i = 0; i < N; i++)
        for (int j = 0; j < N; j++) {
            a[i * N + j] = (i + j) % 10;
            b[i * N + j] = (i * j) % 7;
        }

    for (int i = 0; i < N; i++)
        for (int j = 0; j < N; j++) {
            int s = 0;
            for (int k = 0; k < N; k++) s += a[i * N + k] * b[k * N + j];
            c[i * N + j] = s;
        }

    long long total = 0;
    for (int x = 0; x < N * N; x += 2) total += c[x];
    printf("%lld\n", total);
    free(a);
    free(b);
    free(c);
    return 0;
}
