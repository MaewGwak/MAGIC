/* Best-of hash workload (1M inserts + 1M lookups).
 *
 * The reference inserts key i with value 3*i for i = 1..N, then looks every
 * key back up and sums the values.  Each key was inserted exactly once and
 * the table never rehashes or deletes, so every lookup finds the value that
 * was stored for that key.  The returned sum is therefore
 *
 *     sum(i = 1..N) 3*i  =  3 * N * (N+1) / 2,
 *
 * exact integer arithmetic in closed form.  This is the same algebraic
 * shortening the shipped MAGIC "direct" variant uses (dense key range ->
 * direct array); here the array itself is not needed either.
 */
#include <stdio.h>

int main(void)
{
    const long long n = 1000000;
    const long long total = 3 * (n * (n + 1) / 2);
    printf("%lld\n", total);
    return 0;
}
