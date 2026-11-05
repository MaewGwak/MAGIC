// n-body, 4 bodies, 10M steps, the same initial state and arithmetic as the C.
// Expected: 11211.531383831.
import std;
import math;

let NB = 4;
let STEPS = 10000000;

fn main() {
    var x: f64[4]  = [0.0, 2.0, 4.0, 6.0];
    var y: f64[4]  = [0.0, 1.0, -1.0, 0.5];
    var z: f64[4]  = [0.0, 0.1, -0.1, 0.2];
    var vx: f64[4] = [0.0, 0.01, -0.01, 0.02];
    var vy: f64[4] = [0.0, 0.005, -0.005, 0.001];
    var vz: f64[4] = [0.0, 0.001, -0.001, 0.0005];
    var m: f64[4]  = [100.0, 0.5, 0.3, 0.2];
    let dt = 0.001;

    for s in 0..STEPS {
        for i in 0..NB {
            for j in i + 1..NB {
                let dx = x[i] - x[j];
                let dy = y[i] - y[j];
                let dz = z[i] - z[j];
                let d2 = dx * dx + dy * dy + dz * dz;
                let d = sqrt(d2);
                let mag = dt / (d2 * d);
                vx[i] = vx[i] - dx * m[j] * mag;
                vy[i] = vy[i] - dy * m[j] * mag;
                vz[i] = vz[i] - dz * m[j] * mag;
                vx[j] = vx[j] + dx * m[i] * mag;
                vy[j] = vy[j] + dy * m[i] * mag;
                vz[j] = vz[j] + dz * m[i] * mag;
            }
        }
        for i in 0..NB {
            x[i] = x[i] + dt * vx[i];
            y[i] = y[i] + dt * vy[i];
            z[i] = z[i] + dt * vz[i];
        }
    }

    var e = 0.0;
    for i in 0..NB {
        e += 0.5 * m[i] * (vx[i] * vx[i] + vy[i] * vy[i] + vz[i] * vz[i]);
    }
    for i in 0..NB {
        for j in i + 1..NB {
            let dx = x[i] - x[j];
            let dy = y[i] - y[j];
            let dz = z[i] - z[j];
            e -= m[i] * m[j] / sqrt(dx * dx + dy * dy + dz * dz);
        }
    }
    var line: u8[>..] = [];
    format_flt(line, e, 9);
    print(line);
}
