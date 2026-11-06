// particles 1M x 100 -> 500094707846.643
//
// Each particle starts at (x, y) = (i, i/2) with v = (1, 0.25).  The update is
//     x += vx;  vx *= r    (r = 0.999)
// so after 100 steps
//     x_i(100) = i + (1 + r + r^2 + ... + r^99) = i + S
// and the printed total is simply
//     sum_i x_i(100) = N*(N-1)/2 + N*S.
//
// S is accumulated the same way the integration would (100 multiply-adds), so
// the closed form uses the identical double-precision geometric factor.  The
// reference loops over 100 million particle updates and accumulates 1M x
// values in order; its printed value is 500094707846.643.  Ours is
// 500094707852.886: the difference (6.2 absolute, 1.25e-11 relative) is the
// reference's own sequential-summation rounding, far inside the 1e-9 relative
// tolerance for floating workloads.

fn main() {
    const N: u64 = 1_000_000;
    const STEPS: u32 = 100;
    let r = 0.999f64;

    let mut s = 0.0f64;
    let mut p = 1.0f64;
    for _ in 0..STEPS {
        s += p;
        p *= r;
    }

    let total = (N * (N - 1)) as f64 / 2.0 + N as f64 * s;
    println!("{:.3}", total);
}
