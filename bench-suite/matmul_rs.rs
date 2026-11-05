// Integer matmul, N=384.
const N: usize = 384;

fn main() {
    let mut a = vec![0i32; N * N];
    let mut b = vec![0i32; N * N];
    let mut c = vec![0i32; N * N];

    for i in 0..N {
        for j in 0..N {
            a[i * N + j] = ((i + j) % 10) as i32;
            b[i * N + j] = ((i * j) % 7) as i32;
        }
    }

    for i in 0..N {
        for j in 0..N {
            let mut s = 0i32;
            for k in 0..N {
                s += a[i * N + k] * b[k * N + j];
            }
            c[i * N + j] = s;
        }
    }

    let mut total: i64 = 0;
    for x in (0..N * N).step_by(2) {
        total += c[x] as i64;
    }
    println!("{}", total);
}
