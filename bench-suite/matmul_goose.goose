// Integer matmul, N = 384, the same loops and values as the C version.
// Expected: 326316140.
import std;

let N = 384;

fn main() {
    var a: i32[>..] = [];
    var b: i32[>..] = [];
    var c: i32[>..] = [];
    a.resize(N * N, 0);
    b.resize(N * N, 0);
    c.resize(N * N, 0);

    for i in 0..N {
        for j in 0..N {
            a[i * N + j] = ((i + j) % 10) as! i32;
            b[i * N + j] = ((i * j) % 7) as! i32;
        }
    }

    for i in 0..N {
        for j in 0..N {
            var s: i32 = 0;
            for k in 0..N {
                s += a[i * N + k] * b[k * N + j];
            }
            c[i * N + j] = s;
        }
    }

    var total: i64 = 0;
    var x = 0;
    while x < N * N {
        total += c[x] as! i64;
        x += 2;
    }
    print(total);
}
