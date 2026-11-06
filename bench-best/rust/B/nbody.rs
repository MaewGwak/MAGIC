// n-body, 4 bodies, 10,000,000 steps -> 11211.531383844 (FMA build)
//
// Every step applies the reference's symplectic-Euler update over the six
// unordered pairs and then advances the positions.  The four bodies are kept
// in named f64 locals and the six pair interactions are fully unrolled, so
// nothing spills to memory and the six independent sqrt/div chains overlap.
//
// The inner products use fused multiply-add (mul_add), which is what
// gcc -O3 -march=native contracts by default: the printed energy is exactly
// the FMA reference 11211.531383844.  The non-FMA Rust build prints
// 11211.531383831; the two differ by 1.3e-8 absolute (1.2e-12 relative),
// inside the 1e-9 relative tolerance either way.
//
// The loop-carried chain is sqrt(d2) -> dt/(d2*d) -> v update -> x update;
// FMA removes one dependent add from the position update and the unrolled
// six-pair body gives the scheduler six independent chains per step.

const DT: f64 = 0.001;
const STEPS: u32 = 10_000_000;

macro_rules! pair {
    ($xi:ident,$yi:ident,$zi:ident,$vxi:ident,$vyi:ident,$vzi:ident,$mi:expr,
     $xj:ident,$yj:ident,$zj:ident,$vxj:ident,$vyj:ident,$vzj:ident,$mj:expr) => {{
        let dx = $xi - $xj;
        let dy = $yi - $yj;
        let dz = $zi - $zj;
        let d2 = dx.mul_add(dx, dy.mul_add(dy, dz * dz));
        let d = d2.sqrt();
        let mag = DT / (d2 * d);
        $vxi = (-$mj * dx).mul_add(mag, $vxi);
        $vyi = (-$mj * dy).mul_add(mag, $vyi);
        $vzi = (-$mj * dz).mul_add(mag, $vzi);
        $vxj = ($mi * dx).mul_add(mag, $vxj);
        $vyj = ($mi * dy).mul_add(mag, $vyj);
        $vzj = ($mi * dz).mul_add(mag, $vzj);
    }};
}

fn main() {
    let m0 = 100.0f64;
    let m1 = 0.5f64;
    let m2 = 0.3f64;
    let m3 = 0.2f64;

    let mut x0 = 0.0f64;
    let mut y0 = 0.0f64;
    let mut z0 = 0.0f64;
    let mut x1 = 2.0f64;
    let mut y1 = 1.0f64;
    let mut z1 = 0.1f64;
    let mut x2 = 4.0f64;
    let mut y2 = -1.0f64;
    let mut z2 = -0.1f64;
    let mut x3 = 6.0f64;
    let mut y3 = 0.5f64;
    let mut z3 = 0.2f64;

    let mut vx0 = 0.0f64;
    let mut vy0 = 0.0f64;
    let mut vz0 = 0.0f64;
    let mut vx1 = 0.01f64;
    let mut vy1 = 0.005f64;
    let mut vz1 = 0.001f64;
    let mut vx2 = -0.01f64;
    let mut vy2 = -0.005f64;
    let mut vz2 = -0.001f64;
    let mut vx3 = 0.02f64;
    let mut vy3 = 0.001f64;
    let mut vz3 = 0.0005f64;

    let mut s = 0u32;
    while s < STEPS {
        pair!(x0, y0, z0, vx0, vy0, vz0, m0, x1, y1, z1, vx1, vy1, vz1, m1);
        pair!(x0, y0, z0, vx0, vy0, vz0, m0, x2, y2, z2, vx2, vy2, vz2, m2);
        pair!(x0, y0, z0, vx0, vy0, vz0, m0, x3, y3, z3, vx3, vy3, vz3, m3);
        pair!(x1, y1, z1, vx1, vy1, vz1, m1, x2, y2, z2, vx2, vy2, vz2, m2);
        pair!(x1, y1, z1, vx1, vy1, vz1, m1, x3, y3, z3, vx3, vy3, vz3, m3);
        pair!(x2, y2, z2, vx2, vy2, vz2, m2, x3, y3, z3, vx3, vy3, vz3, m3);

        x0 = DT.mul_add(vx0, x0);
        y0 = DT.mul_add(vy0, y0);
        z0 = DT.mul_add(vz0, z0);
        x1 = DT.mul_add(vx1, x1);
        y1 = DT.mul_add(vy1, y1);
        z1 = DT.mul_add(vz1, z1);
        x2 = DT.mul_add(vx2, x2);
        y2 = DT.mul_add(vy2, y2);
        z2 = DT.mul_add(vz2, z2);
        x3 = DT.mul_add(vx3, x3);
        y3 = DT.mul_add(vy3, y3);
        z3 = DT.mul_add(vz3, z3);

        s += 1;
    }

    let mut e = 0.0f64;
    e += 0.5 * m0 * (vx0 * vx0 + vy0 * vy0 + vz0 * vz0);
    e += 0.5 * m1 * (vx1 * vx1 + vy1 * vy1 + vz1 * vz1);
    e += 0.5 * m2 * (vx2 * vx2 + vy2 * vy2 + vz2 * vz2);
    e += 0.5 * m3 * (vx3 * vx3 + vy3 * vy3 + vz3 * vz3);

    macro_rules! potential {
        ($xi:ident,$yi:ident,$zi:ident,$mi:expr,$xj:ident,$yj:ident,$zj:ident,$mj:expr) => {{
            let dx = $xi - $xj;
            let dy = $yi - $yj;
            let dz = $zi - $zj;
            e -= $mi * $mj / (dx * dx + dy * dy + dz * dz).sqrt();
        }};
    }
    potential!(x0, y0, z0, m0, x1, y1, z1, m1);
    potential!(x0, y0, z0, m0, x2, y2, z2, m2);
    potential!(x0, y0, z0, m0, x3, y3, z3, m3);
    potential!(x1, y1, z1, m1, x2, y2, z2, m2);
    potential!(x1, y1, z1, m1, x3, y3, z3, m3);
    potential!(x2, y2, z2, m2, x3, y3, z3, m3);

    println!("{:.9}", e);
}
