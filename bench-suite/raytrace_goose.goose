// Ray tracer, 1536x1536, 5 spheres: the same scene and arithmetic as the C.
// Expected: 336141.941.
import std;
import math;

let W = 1536;
let H = 1536;
let S = 5;

fn main() {
    var cx: f64[5] = [0.0, 1.5, -1.2, 2.5, -2.0];
    var cy: f64[5] = [0.0, 0.3, -0.6, -0.8, 1.2];
    var cz: f64[5] = [3.0, 3.2, 2.8, 4.0, 3.5];
    var rr: f64[5] = [1.0, 0.6, 0.7, 0.5, 0.4];
    let lx = 5.0;
    let ly = 5.0;
    let lz = 0.0;

    var total = 0.0;
    for y in 0..H {
        for x in 0..W {
            let u = (x as f64 - 768.0) / 768.0;
            let v = (y as f64 - 768.0) / 768.0;
            var dx = u;
            var dy = v;
            var dz = 1.5;
            let len = sqrt(dx * dx + dy * dy + dz * dz);
            dx = dx / len;
            dy = dy / len;
            dz = dz / len;

            var best = 1000000000.0;
            var hit: i64 = -1;
            for s in 0..S {
                let ox = -cx[s];
                let oy = -cy[s];
                let oz = -cz[s];
                let b = ox * dx + oy * dy + oz * dz;
                let c = ox * ox + oy * oy + oz * oz - rr[s] * rr[s];
                let disc = b * b - c;
                if disc > 0 {
                    let t = -b - sqrt(disc);
                    if t > 0 && t < best {
                        best = t;
                        hit = s;
                    }
                }
            }

            var col = 0.0;
            if hit >= 0 {
                let h = hit as! i64;
                let px = dx * best;
                let py = dy * best;
                let pz = dz * best;
                var nx = px - cx[h];
                var ny = py - cy[h];
                var nz = pz - cz[h];
                let nl = sqrt(nx * nx + ny * ny + nz * nz);
                nx = nx / nl;
                ny = ny / nl;
                nz = nz / nl;
                let ddx = lx - px;
                let ddy = ly - py;
                let ddz = lz - pz;
                let ll = sqrt(ddx * ddx + ddy * ddy + ddz * ddz);
                var d = (nx * ddx + ny * ddy + nz * ddz) / ll;
                if d < 0 {
                    d = 0.0;
                }
                col = d;
            }
            total += col;
        }
    }
    var line: u8[>..] = [];
    format_flt(line, total, 3);
    print(line);
}
