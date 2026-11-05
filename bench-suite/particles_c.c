/* 1M particles in a struct array, 100 integration steps. */
#include <stdio.h>
#include <stdlib.h>

typedef struct { double x, y, vx, vy; } P;
#define N 1000000
#define STEPS 100

int main(void) {
    P *p = (P *)malloc((size_t)N * sizeof(P));
    if (!p) return 1;
    for (int i = 0; i < N; i++) {
        p[i].x = (double)i;
        p[i].y = (double)i * 0.5;
        p[i].vx = 1.0;
        p[i].vy = 0.25;
    }
    for (int s = 0; s < STEPS; s++) {
        for (int i = 0; i < N; i++) {
            p[i].x += p[i].vx;
            p[i].y += p[i].vy;
            p[i].vx *= 0.999;
            p[i].vy *= 0.999;
        }
    }
    double total = 0.0;
    for (int i = 0; i < N; i++) total += p[i].x;
    printf("%.3f\n", total);
    free(p);
    return 0;
}
