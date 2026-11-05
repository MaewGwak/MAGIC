// 1M particles in a Vec, 100 integration steps.
#[derive(Clone, Copy)]
struct P {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
}

fn main() {
    const N: usize = 1_000_000;
    const STEPS: usize = 100;
    let mut p = vec![P { x: 0.0, y: 0.0, vx: 0.0, vy: 0.0 }; N];
    for i in 0..N {
        p[i].x = i as f64;
        p[i].y = i as f64 * 0.5;
        p[i].vx = 1.0;
        p[i].vy = 0.25;
    }
    for _ in 0..STEPS {
        for i in 0..N {
            p[i].x += p[i].vx;
            p[i].y += p[i].vy;
            p[i].vx *= 0.999;
            p[i].vy *= 0.999;
        }
    }
    let mut total = 0.0;
    for i in 0..N {
        total += p[i].x;
    }
    println!("{:.3}", total);
}
