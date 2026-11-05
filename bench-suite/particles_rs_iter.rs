// Same particle workload, written the idiomatic Rust way: iter_mut (no bounds checks, vectorizable).
struct P {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
}

fn main() {
    const N: usize = 1_000_000;
    const STEPS: usize = 100;
    let mut p: Vec<P> = (0..N)
        .map(|i| P {
            x: i as f64,
            y: i as f64 * 0.5,
            vx: 1.0,
            vy: 0.25,
        })
        .collect();

    for _ in 0..STEPS {
        for q in p.iter_mut() {
            q.x += q.vx;
            q.y += q.vy;
            q.vx *= 0.999;
            q.vy *= 0.999;
        }
    }

    let mut total = 0.0;
    for q in &p {
        total += q.x;
    }
    println!("{:.3}", total);
}
