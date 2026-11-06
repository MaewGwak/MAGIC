fn main(){
    let n: i64 = 1_000_000; let steps = 100; let r = 0.999f64;
    let geom = (1.0 - r.powi(steps)) / (1.0 - r);
    let total = (n * (n - 1)) as f64 / 2.0 + n as f64 * geom;
    println!("{:.3}", total);
}
