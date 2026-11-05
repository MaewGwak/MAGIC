/* Buffer pool: 512 pages of 4 KB, 64 frames, LRU, 200k random accesses. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>

#define PAGE 4096
#define NPAGES 512
#define FRAMES 64
#define ACCESSES 200000

int main(void) {
    uint8_t *page_buf = malloc(PAGE);
    FILE *w = fopen("bench-db/pages.dat", "wb");
    if (!w) return 1;
    for (int p = 0; p < NPAGES; p++) {
        for (int j = 0; j < PAGE; j++) page_buf[j] = (uint8_t)((p * 31 + j) % 256);
        fwrite(page_buf, 1, PAGE, w);
    }
    fclose(w);

    uint8_t *frame_data = calloc(FRAMES, PAGE);
    int32_t fpage[FRAMES], fpin[FRAMES] = {0};
    int64_t fused[FRAMES] = {0};
    for (int k = 0; k < FRAMES; k++) fpage[k] = -1;

    FILE *f = fopen("bench-db/pages.dat", "rb");
    if (!f || !frame_data) return 1;
    int64_t hits = 0, misses = 0, checksum = 0;
    int64_t x = 12345;
    for (int t = 0; t < ACCESSES; t++) {
        x = (x * 1103515245 + 12345) % 2147483648LL;
        int32_t id = (int32_t)((x / 65536) % NPAGES);
        int fnum = -1;
        for (int k = 0; k < FRAMES; k++) if (fpage[k] == id) fnum = k;
        if (fnum >= 0) {
            hits++;
            fused[fnum] = t;
            checksum += frame_data[fnum * PAGE];
        } else {
            misses++;
            int v = 0;
            int64_t best = fused[0];
            for (int k = 1; k < FRAMES; k++) if (fused[k] < best) { best = fused[k]; v = k; }
            fseek(f, (long)id * PAGE, SEEK_SET);
            fread(frame_data + v * PAGE, 1, PAGE, f);
            fpage[v] = id;
            fused[v] = t;
            checksum += frame_data[v * PAGE];
        }
    }
    fclose(f);
    printf("%lld\n%lld\n%lld\n", (long long)hits, (long long)misses, (long long)checksum);
    return 0;
}
