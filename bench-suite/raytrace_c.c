/* Ray tracer, 256x256, 5 spheres. Prints total brightness. */
#include <stdio.h>
#include <math.h>
#define W 1536
#define H 1536
#define S 5

int main(void) {
    double cx[S] = {0.0, 1.5, -1.2, 2.5, -2.0};
    double cy[S] = {0.0, 0.3, -0.6, -0.8, 1.2};
    double cz[S] = {3.0, 3.2, 2.8, 4.0, 3.5};
    double rr[S] = {1.0, 0.6, 0.7, 0.5, 0.4};
    double lx = 5.0, ly = 5.0, lz = 0.0;

    double total = 0.0;
    for (int y = 0; y < H; y++) {
        for (int x = 0; x < W; x++) {
            double u = (x - W / 2.0) / (W / 2.0);
            double v = (y - H / 2.0) / (H / 2.0);
            double dx = u, dy = v, dz = 1.5;
            double len = sqrt(dx * dx + dy * dy + dz * dz);
            dx /= len; dy /= len; dz /= len;

            double best = 1000000000.0;
            int hit = -1;
            for (int s = 0; s < S; s++) {
                double ox = -cx[s], oy = -cy[s], oz = -cz[s];
                double b = ox * dx + oy * dy + oz * dz;
                double c = ox * ox + oy * oy + oz * oz - rr[s] * rr[s];
                double disc = b * b - c;
                if (disc > 0) {
                    double t = -b - sqrt(disc);
                    if (t > 0 && t < best) { best = t; hit = s; }
                }
            }

            double col = 0.0;
            if (hit >= 0) {
                double px = dx * best, py = dy * best, pz = dz * best;
                double nx = px - cx[hit], ny = py - cy[hit], nz = pz - cz[hit];
                double nl = sqrt(nx * nx + ny * ny + nz * nz);
                nx /= nl; ny /= nl; nz /= nl;
                double ldx = lx - px, ldy = ly - py, ldz = lz - pz;
                double ll = sqrt(ldx * ldx + ldy * ldy + ldz * ldz);
                double d = (nx * ldx + ny * ldy + nz * ldz) / ll;
                if (d < 0) d = 0;
                col = d;
            }
            total += col;
        }
    }
    printf("%.3f\n", total);
    return 0;
}
