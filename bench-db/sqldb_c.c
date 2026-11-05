/* mini-PostgreSQL: tokenizer + parser + executor, mirroring sqldb_magic.magic. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>
#include <string.h>

static bool is_digit(uint8_t c) { return c >= 48 && c <= 57; }
static bool is_alpha(uint8_t c) {
    return (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c == 95;
}
static uint8_t upper(uint8_t c) { return (c >= 97 && c <= 122) ? (uint8_t)(c - 32) : c; }

typedef struct { int32_t kind, start, len; int64_t val; } Tok;

static bool eq_word(const uint8_t *buf, const Tok *t, const char *s) {
    int32_t n = t->len;
    if (n != (int32_t)strlen(s)) return false;
    for (int32_t i = 0; i < n; i++) {
        if (upper(buf[t->start + i]) != upper((uint8_t)s[i])) return false;
    }
    return true;
}

int main(void) {
    const size_t CAP = 8388608;
    uint8_t *buf = malloc(CAP);
    FILE *f = fopen("bench-db/sqldb.dat", "rb");
    if (!buf || !f) return 1;
    size_t n = fread(buf, 1, CAP, f);
    fclose(f);

    Tok *tok = malloc(2000000 * sizeof(Tok));
    int32_t nt = 0;
    size_t i = 0;
    while (i < n) {
        uint8_t c = buf[i];
        if (c == 32 || c == 10 || c == 13 || c == 9) {
            i++;
        } else if (is_digit(c)) {
            int64_t v = 0;
            size_t s0 = i;
            while (i < n && is_digit(buf[i])) { v = v * 10 + (int64_t)(buf[i] - 48); i++; }
            tok[nt++] = (Tok){1, (int32_t)s0, (int32_t)(i - s0), v};
        } else if (is_alpha(c)) {
            size_t s0 = i;
            while (i < n && (is_alpha(buf[i]) || is_digit(buf[i]))) i++;
            tok[nt++] = (Tok){0, (int32_t)s0, (int32_t)(i - s0), 0};
        } else {
            tok[nt++] = (Tok){2, (int32_t)i, 1, 0};
            i++;
        }
    }

    int64_t *rk = malloc(200000 * sizeof(int64_t));
    int64_t *rv = malloc(200000 * sizeof(int64_t));
    int64_t rows = 0, checksum = 0;
    int32_t p = 0;
    while (p < nt) {
        if (tok[p].kind == 0 && eq_word(buf, &tok[p], "INSERT")) {
            rk[rows] = tok[p + 5].val;
            rv[rows] = tok[p + 7].val;
            rows++;
            p += 9;
        } else if (tok[p].kind == 0 && eq_word(buf, &tok[p], "SELECT")) {
            int32_t q = p + 1;
            if (tok[q].kind == 0 && eq_word(buf, &tok[q], "COUNT")) {
                checksum += rows;
                p = q + 6;
            } else if (tok[q].kind == 0 && eq_word(buf, &tok[q], "SUM")) {
                int64_t total = 0;
                for (int64_t r = 0; r < rows; r++) total += rv[r];
                checksum += total;
                p = q + 6;
            } else {
                int64_t key = tok[p + 7].val;
                int64_t found = 0;
                for (int64_t r = 0; r < rows; r++) if (rk[r] == key) found = rv[r];
                checksum += found;
                p += 9;
            }
        } else {
            p++;
        }
    }
    printf("%lld\n%lld\n", (long long)rows, (long long)checksum);
    return 0;
}
