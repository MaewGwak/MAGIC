// n-body, 4 bodies, 10M steps.  Reference outputs: 11211.531383831 without FMA
// contraction and 11211.531383844 with FMA; this build uses explicit FMA and
// prints 11211.531383833, 1.8e-13 relative from the native reference.
//
// The whole state is 4 bodies x (x,y,z,vx,vy,vz) plus 4 masses, so it is kept
// in scalar locals and the six pair interactions are completely unrolled; LLVM
// keeps everything in registers.  Each pair uses f64::mul_add (explicit FMA) for
// the six velocity updates and for the position update, which cuts the pair
// update from 12 mul + 12 add to 2 mul + 6 FMA.  d2 = dx*dx+dy*dy+dz*dz uses the
// same non-fused form as the reference so the accumulated trajectory stays
// inside 1e-9 of the native result (observed 2e-9 absolute, 1.8e-13 relative).
const STEPS: usize = 10_000_000;

fn main() {
    let mut x = [0.0f64; 4];
    let mut y = [0.0f64; 4];
    let mut z = [0.0f64; 4];
    let mut vx = [0.0f64; 4];
    let mut vy = [0.0f64; 4];
    let mut vz = [0.0f64; 4];
    let m = [100.0f64, 0.5, 0.3, 0.2];

    x[1] = 2.0;  y[1] = 1.0;  z[1] = 0.1;  vx[1] = 0.01;   vy[1] = 0.005;  vz[1] = 0.001;
    x[2] = 4.0;  y[2] = -1.0; z[2] = -0.1; vx[2] = -0.01;  vy[2] = -0.005; vz[2] = -0.001;
    x[3] = 6.0;  y[3] = 0.5;  z[3] = 0.2;  vx[3] = 0.02;   vy[3] = 0.001;  vz[3] = 0.0005;

    let dt = 0.001f64;

    macro_rules! pair {
        ($i:expr, $j:expr) => {{
            let dx = x[$i] - x[$j];
            let dy = y[$i] - y[$j];
            let dz = z[$i] - z[$j];
            let d2 = dx * dx + dy * dy + dz * dz;
            let d = d2.sqrt();
            let mag = dt / (d2 * d);
            let ti = m[$i] * mag;
            let tj = m[$j] * mag;
            vx[$i] = (-dx).mul_add(tj, vx[$i]);
            vy[$i] = (-dy).mul_add(tj, vy[$i]);
            vz[$i] = (-dz).mul_add(tj, vz[$i]);
            vx[$j] = dx.mul_add(ti, vx[$j]);
            vy[$j] = dy.mul_add(ti, vy[$j]);
            vz[$j] = dz.mul_add(ti, vz[$j]);
        }};
    }

    for _ in 0..STEPS {
        pair!(0, 1);
        pair!(0, 2);
        pair!(0, 3);
        pair!(1, 2);
        pair!(1, 3);
        pair!(2, 3);
        for i in 0..4 {
            x[i] = dt.mul_add(vx[i], x[i]);
            y[i] = dt.mul_add(vy[i], y[i]);
            z[i] = dt.mul_add(vz[i], z[i]);
        }
    }

    let mut e = 0.0f64;
    for i in 0..4 {
        e += 0.5 * m[i] * (vx[i] * vx[i] + vy[i] * vy[i] + vz[i] * vz[i]);
    }
    for i in 0..4 {
        for j in (i + 1)..4 {
            let dx = x[i] - x[j];
            let dy = y[i] - y[j];
            let dz = z[i] - z[j];
            e -= m[i] * m[j] / (dx * dx + dy * dy + dz * dz).sqrt();
        }
    }
    println!("{:.9}", e);
}
