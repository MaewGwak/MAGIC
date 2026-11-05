/* SQL tokenizer over a byte buffer, mirroring sqlscan_magic.magic. */
#include <stdio.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdlib.h>

static bool is_digit(uint8_t c) { return c >= 48 && c <= 57; }
static bool is_alpha(uint8_t c) {
    return (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c == 95;
}

int main(void) {
    const size_t CAP = 8388608;
    uint8_t *buf = malloc(CAP);
    FILE *f = fopen("bench-db/sql.dat", "rb");
    if (!f || !buf) return 1;
    size_t n = fread(buf, 1, CAP, f);
    fclose(f);

    size_t i = 0;
    int64_t tokens = 0, sum = 0;
    while (i < n) {
        uint8_t c = buf[i];
        if (c == 32 || c == 10 || c == 13 || c == 9) {
            i++;
        } else if (is_digit(c)) {
            int64_t v = 0;
            while (i < n && is_digit(buf[i])) {
                v = v * 10 + (int64_t)(buf[i] - 48);
                i++;
            }
            tokens++;
            sum += v;
        } else if (is_alpha(c)) {
            while (i < n && (is_alpha(buf[i]) || is_digit(buf[i]))) i++;
            tokens++;
        } else {
            i++;
            tokens++;
        }
    }
    printf("%lld\n%lld\n", (long long)tokens, (long long)sum);
    return 0;
}
