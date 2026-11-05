// Ray tracer, 512x512, 5 spheres.
fn main() {
    let cx = [0.0, 1.5, -1.2, 2.5, -2.0];
    let cy = [0.0, 0.3, -0.6, -0.8, 1.2];
    let cz = [3.0, 3.2, 2.8, 4.0, 3.5];
    let rr = [1.0, 0.6, 0.7, 0.5, 0.4];
    let (lx, ly, lz) = (5.0, 5.0, 0.0);

    let mut total = 0.0;
    for y in 0..1536 {
        for x in 0..1536 {
            let u = (x as f64 - 768.0) / 768.0;
            let v = (y as f64 - 768.0) / 768.0;
            let (mut dx, mut dy, mut dz) = (u, v, 1.5);
            let len = (dx * dx + dy * dy + dz * dz).sqrt();
            dx /= len;
            dy /= len;
            dz /= len;

            let mut best = 1_000_000_000.0;
            let mut hit: i32 = -1;
            for s in 0..5 {
                let ox = -cx[s];
                let oy = -cy[s];
                let oz = -cz[s];
                let b = ox * dx + oy * dy + oz * dz;
                let c = ox * ox + oy * oy + oz * oz - rr[s] * rr[s];
                let disc = b * b - c;
                if disc > 0.0 {
                    let t = -b - disc.sqrt();
                    if t > 0.0 && t < best {
                        best = t;
                        hit = s as i32;
                    }
                }
            }

            let mut col = 0.0;
            if hit >= 0 {
                let s = hit as usize;
                let px = dx * best;
                let py = dy * best;
                let pz = dz * best;
                let mut nx = px - cx[s];
                let mut ny = py - cy[s];
                let mut nz = pz - cz[s];
                let nl = (nx * nx + ny * ny + nz * nz).sqrt();
                nx /= nl;
                ny /= nl;
                nz /= nl;
                let ldx = lx - px;
                let ldy = ly - py;
                let ldz = lz - pz;
                let ll = (ldx * ldx + ldy * ldy + ldz * ldz).sqrt();
                let mut d = (nx * ldx + ny * ldy + nz * ldz) / ll;
                if d < 0.0 {
                    d = 0.0;
                }
                col = d;
            }
            total += col;
        }
    }
    println!("{:.3}", total);
}
