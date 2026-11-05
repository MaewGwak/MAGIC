/* fannkuch-redux, n = 10. Reference outputs: 73196 and 38. */
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    const int n = 10;
    int *perm1 = (int *)malloc((size_t)n * sizeof(int));
    int *perm  = (int *)malloc((size_t)n * sizeof(int));
    int *count = (int *)malloc((size_t)n * sizeof(int));
    if (!perm1 || !perm || !count) return 1;

    for (int i = 0; i < n; i++) perm1[i] = i;
    int r = n;
    int max_flips = 0;
    int checksum = 0;
    int perm_count = 0;

    for (;;) {
        while (r != 1) {
            count[r - 1] = r;
            r--;
        }
        for (int i = 0; i < n; i++) perm[i] = perm1[i];

        int flips = 0;
        while (perm[0] != 0) {
            int k = perm[0];
            for (int i = 0, j = k; i < j; i++, j--) {
                int t = perm[i];
                perm[i] = perm[j];
                perm[j] = t;
            }
            flips++;
        }
        if (flips > max_flips) max_flips = flips;
        checksum += (perm_count % 2 == 0) ? flips : -flips;
        perm_count++;

        for (;;) {
            if (r == n) {
                printf("%d\n%d\n", checksum, max_flips);
                free(perm1);
                free(perm);
                free(count);
                return 0;
            }
            int p0 = perm1[0];
            for (int i = 0; i < r; i++) perm1[i] = perm1[i + 1];
            perm1[r] = p0;
            if (--count[r] > 0) break;
            r++;
        }
    }
}
