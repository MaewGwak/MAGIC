// 1M particles in a struct array, 100 integration steps, as in the C.
// Expected: 500094707846.643.
import std;

let N = 1000000;
let STEPS = 100;

struct P { x: f64, y: f64, vx: f64, vy: f64 }

fn main() {
    var p: P[>..] = [];
    p.resize(N, P { x: 0.0, y: 0.0, vx: 0.0, vy: 0.0 });
    for i in 0..N {
        p[i].x = i as f64;
        p[i].y = (i as f64) * 0.5;
        p[i].vx = 1.0;
        p[i].vy = 0.25;
    }
    for s in 0..STEPS {
        for i in 0..N {
            p[i].x += p[i].vx;
            p[i].y += p[i].vy;
            p[i].vx *= 0.999;
            p[i].vy *= 0.999;
        }
    }
    var total = 0.0;
    for i in 0..N {
        total += p[i].x;
    }
    var line: u8[>..] = [];
    format_flt(line, total, 3);
    print(line);
}
