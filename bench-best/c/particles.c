/* Particles 1M x 100 steps, constant velocity decay.
 *
 * Each particle follows x_{s+1} = x_s + v_s, v_{s+1} = v_s * r with constant
 * r = 0.999, so x_100 = x_0 + (1 + r + ... + r^99).  The 100-step integration
 * collapses to one geometric series (the owner-approved MAGIC variant does the
 * same), and the only remaining work is the final reduction.
 *
 * The final sum is deliberately accumulated in the original order: the running
 * total reaches 5e14 where one ulp is 0.0625, so the reference's left-to-right
 * summation has a reproducible ~6.2 absolute rounding drift.  Summing
 * n(n-1)/2 + n*ssum instead gives 500094707852.886 (1.2e-11 relative away,
 * within the 1e-9 floating tolerance) but not the exact reference digits, so
 * the O(N) reduction is kept.  It is still ~100x faster than the 100 passes.
 *
 * Expected: 500094707846.643
 */
#include <stdio.h>

int main(void) {
    const int n = 1000000;
    const int steps = 100;
    const double r = 0.999;

    double ssum = 0.0, p = 1.0;
    for (int k = 0; k < steps; k++) {
        ssum += p;
        p *= r;
    }

    double total = 0.0;
    for (int i = 0; i < n; i++) total += (double)i + ssum;

    printf("%.3f\n", total);
    return 0;
}
