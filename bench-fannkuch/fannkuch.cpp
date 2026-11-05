// fannkuch-redux, n = 10. Reference outputs: 73196 and 38.
#include <cstdio>
#include <vector>

int main() {
    const int n = 10;
    std::vector<int> perm1(n), perm(n), count(n);
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
        perm = perm1;

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
                std::printf("%d\n%d\n", checksum, max_flips);
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

