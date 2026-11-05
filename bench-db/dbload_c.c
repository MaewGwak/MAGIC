/* Bulk loader: INSERT statements -> heap file (8-byte row count, 16-byte rows). */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

static bool is_digit(uint8_t c) { return c >= 48 && c <= 57; }
static void put_i64(uint8_t *p, int64_t v) {
    for (int k = 0; k < 8; k++) { p[k] = (uint8_t)(v & 255); v >>= 8; }
}

int main(void) {
    const size_t CAP = 8388608;
    uint8_t *sql = malloc(CAP);
    FILE *f = fopen("bench-db/inserts.dat", "rb");
    if (!sql || !f) return 1;
    size_t n = fread(sql, 1, CAP, f);
    fclose(f);

    uint8_t page[4096] = {0};
    FILE *out = fopen("bench-db/heap.dat", "wb");
    if (!out) return 1;
    fwrite(page, 1, 8, out);  /* header placeholder */
    size_t off = 0;
    int64_t rows = 0, k0 = 0;
    int have = 0;
    size_t i = 0;
    while (i < n) {
        if (is_digit(sql[i])) {
            int64_t v = 0;
            while (i < n && is_digit(sql[i])) { v = v * 10 + (int64_t)(sql[i] - 48); i++; }
            if (!have) { k0 = v; have = 1; }
            else {
                put_i64(page + off, k0);
                put_i64(page + off + 8, v);
                off += 16; rows++; have = 0;
                if (off == 4096) { fwrite(page, 1, 4096, out); off = 0; }
            }
        } else {
            i++;
        }
    }
    if (off) fwrite(page, 1, 4096, out);
    fseek(out, 0, SEEK_SET);
    uint8_t hdr[8];
    put_i64(hdr, rows);
    fwrite(hdr, 1, 8, out);
    fclose(out);
    printf("%lld\n", (long long)rows);
    return 0;
}
