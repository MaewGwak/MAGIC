/* n-body, 4 bodies, 500k steps. Prints total energy. */
#include <stdio.h>
#include <math.h>
#define NB 4
#define STEPS 10000000

int main(void) {
    double x[NB], y[NB], z[NB], vx[NB], vy[NB], vz[NB], m[NB];

    x[0] = 0;    y[0] = 0;    z[0] = 0;     vx[0] = 0;      vy[0] = 0;      vz[0] = 0;       m[0] = 100.0;
    x[1] = 2;    y[1] = 1;    z[1] = 0.1;   vx[1] = 0.01;   vy[1] = 0.005;  vz[1] = 0.001;   m[1] = 0.5;
    x[2] = 4;    y[2] = -1;   z[2] = -0.1;  vx[2] = -0.01;  vy[2] = -0.005; vz[2] = -0.001;  m[2] = 0.3;
    x[3] = 6;    y[3] = 0.5;  z[3] = 0.2;   vx[3] = 0.02;   vy[3] = 0.001;  vz[3] = 0.0005;  m[3] = 0.2;

    const double dt = 0.001;

    for (int s = 0; s < STEPS; s++) {
        for (int i = 0; i < NB; i++) {
            for (int j = i + 1; j < NB; j++) {
                double dx = x[i] - x[j];
                double dy = y[i] - y[j];
                double dz = z[i] - z[j];
                double d2 = dx * dx + dy * dy + dz * dz;
                double d = __builtin_sqrt(d2);
                double mag = dt / (d2 * d);
                vx[i] -= dx * m[j] * mag;
                vy[i] -= dy * m[j] * mag;
                vz[i] -= dz * m[j] * mag;
                vx[j] += dx * m[i] * mag;
                vy[j] += dy * m[i] * mag;
                vz[j] += dz * m[i] * mag;
            }
        }
        for (int i = 0; i < NB; i++) {
            x[i] += dt * vx[i];
            y[i] += dt * vy[i];
            z[i] += dt * vz[i];
        }
    }

    double e = 0;
    for (int i = 0; i < NB; i++) {
        e += 0.5 * m[i] * (vx[i] * vx[i] + vy[i] * vy[i] + vz[i] * vz[i]);
    }
    for (int i = 0; i < NB; i++) {
        for (int j = i + 1; j < NB; j++) {
            double dx = x[i] - x[j];
            double dy = y[i] - y[j];
            double dz = z[i] - z[j];
            e -= m[i] * m[j] / __builtin_sqrt(dx * dx + dy * dy + dz * dz);
        }
    }
    printf("%.9f\n", e);
    return 0;
}
