// Binary trees: the same perfect trees and node count as the C version.
// Expected: 568328208.
import std;

let MAXN = 2097152;

recursive fn build(left: i32[>..]&, right: i32[>..]&, next: i32[>..]&, depth: i64) -> i64 {
    let idx = next[0] as! i64;
    next[0] = (idx + 1) as! i32;
    if depth > 0 {
        left[idx] = build(left, right, next, depth - 1) as! i32;
        right[idx] = build(left, right, next, depth - 1) as! i32;
    } else {
        left[idx] = -1;
        right[idx] = -1;
    }
    return idx;
}

fn main() {
    var left: i32[>..] = [];
    var right: i32[>..] = [];
    left.resize(MAXN, 0);
    right.resize(MAXN, 0);
    var next: i32[>..] = [0];

    var total: i64 = 0;
    var d = 4;
    while d <= 20 {
        let iters = 1 << (20 - d + 4);
        var i = 0;
        while i < iters {
            next[0] = 0;
            build(left, right, next, d);
            total += next[0] as i64;
            i += 1;
        }
        d += 1;
    }
    print(total);
}
