/* Dynamic array of structs: push 500k, 50 steps, sum. */
#include <stdio.h>
#include <stdlib.h>

typedef struct { double x, y, vx, vy; } P;
#define N 500000
#define STEPS 50

int main(void) {
    size_t cap = N, len = 0;
    P *p = (P *)malloc(cap * sizeof(P));
    if (!p) return 1;
    for (int i = 0; i < N; i++) {
        p[len].x = (double)i;
        p[len].y = (double)i * 0.5;
        p[len].vx = 1.0;
        p[len].vy = 0.25;
        len++;
    }
    for (int s = 0; s < STEPS; s++)
        for (size_t i = 0; i < len; i++) {
            p[i].x += p[i].vx;
            p[i].y += p[i].vy;
            p[i].vx *= 0.999;
            p[i].vy *= 0.999;
        }
    double total = 0.0;
    for (size_t i = 0; i < len; i++) total += p[i].x;
    printf("%.3f\n", total);
    free(p);
    return 0;
}
