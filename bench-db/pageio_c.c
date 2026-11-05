/* Page-store baseline: write 50k pages of 4 KB, read them back, checksum. */
#include <stdio.h>
#include <stdint.h>
#define PAGES 50000
#define PAGE 4096

int main(void) {
    uint8_t buf[PAGE];
    for (int i = 0; i < PAGE; i++) buf[i] = 0;
    FILE *f = fopen("bench-db/pageio.dat", "wb");
    if (!f) return 1;
    for (int i = 0; i < PAGES; i++) {
        buf[0] = (uint8_t)(i % 256);
        if (fwrite(buf, 1, PAGE, f) != PAGE) return 1;
    }
    fclose(f);

    f = fopen("bench-db/pageio.dat", "rb");
    if (!f) return 1;
    int64_t total = 0;
    for (int i = 0; i < PAGES; i++) {
        size_t n = fread(buf, 1, PAGE, f);
        for (size_t j = 0; j < n; j++) total += buf[j];
    }
    fclose(f);
    printf("%lld\n", (long long)total);
    return 0;
}
