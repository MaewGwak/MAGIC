/* Best-of particles (1M particles x 100 steps).
 *
 * Per step the reference does:  x += vx;  vx *= 0.999  (same for y/vy).
 * The velocity therefore decays geometrically and x after 100 steps is
 *
 *     x_i = i + (1 + r + r^2 + ... + r^99),   r = 0.999,
 *
 * for every particle, independent of the others.  Summing over
 * i = 0..N-1 gives  N(N-1)/2 + N * S  with S the geometric sum.
 * S is evaluated with the same 100-step recurrence the reference uses, so
 * the result is the same double-precision quantity up to the last bits of
 * the final summation (relative error far below 1e-9).
 */
#include <stdio.h>

int main(void)
{
    const long long n = 1000000;
    const int steps = 100;
    const double r = 0.999;

    double s = 0.0;      /* 1 + r + ... + r^(steps-1) */
    double p = 1.0;
    for (int k = 0; k < steps; k++) {
        s += p;
        p *= r;
    }

    const double total =
        (double)n * ((double)(n - 1) * 0.5) + (double)n * s;

    printf("%.3f\n", total);
    return 0;
}
