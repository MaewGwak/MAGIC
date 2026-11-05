// n-body, 4 bodies, 500k steps. Prints total energy.
const NB: usize = 4;
const STEPS: usize = 10_000_000;

fn main() {
    let mut x = [0.0f64; NB];
    let mut y = [0.0f64; NB];
    let mut z = [0.0f64; NB];
    let mut vx = [0.0f64; NB];
    let mut vy = [0.0f64; NB];
    let mut vz = [0.0f64; NB];
    let mut m = [0.0f64; NB];

    x[0] = 0.0;   y[0] = 0.0;   z[0] = 0.0;    vx[0] = 0.0;      vy[0] = 0.0;      vz[0] = 0.0;      m[0] = 100.0;
    x[1] = 2.0;   y[1] = 1.0;   z[1] = 0.1;    vx[1] = 0.01;     vy[1] = 0.005;    vz[1] = 0.001;    m[1] = 0.5;
    x[2] = 4.0;   y[2] = -1.0;  z[2] = -0.1;   vx[2] = -0.01;    vy[2] = -0.005;   vz[2] = -0.001;   m[2] = 0.3;
    x[3] = 6.0;   y[3] = 0.5;   z[3] = 0.2;    vx[3] = 0.02;     vy[3] = 0.001;    vz[3] = 0.0005;   m[3] = 0.2;

    let dt = 0.001;

    for _ in 0..STEPS {
        for i in 0..NB {
            for j in (i + 1)..NB {
                let dx = x[i] - x[j];
                let dy = y[i] - y[j];
                let dz = z[i] - z[j];
                let d2 = dx * dx + dy * dy + dz * dz;
                let d = d2.sqrt();
                let mag = dt / (d2 * d);
                vx[i] -= dx * m[j] * mag;
                vy[i] -= dy * m[j] * mag;
                vz[i] -= dz * m[j] * mag;
                vx[j] += dx * m[i] * mag;
                vy[j] += dy * m[i] * mag;
                vz[j] += dz * m[i] * mag;
            }
        }
        for i in 0..NB {
            x[i] += dt * vx[i];
            y[i] += dt * vy[i];
            z[i] += dt * vz[i];
        }
    }

    let mut e = 0.0;
    for i in 0..NB {
        e += 0.5 * m[i] * (vx[i] * vx[i] + vy[i] * vy[i] + vz[i] * vz[i]);
    }
    for i in 0..NB {
        for j in (i + 1)..NB {
            let dx = x[i] - x[j];
            let dy = y[i] - y[j];
            let dz = z[i] - z[j];
            e -= m[i] * m[j] / (dx * dx + dy * dy + dz * dz).sqrt();
        }
    }
    println!("{:.9}", e);
}
