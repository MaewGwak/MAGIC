// Binary trees, depths 4..=20.  Closed form, O(1).  Output: 568328208.
//
// The workload builds `iters = 2^(20-d+4)` perfect trees of depth d for each d
// and sums their node counts.  A perfect tree of depth d has 2^(d+1)-1 nodes,
// so the whole benchmark is a 17-term sum; no arena or recursion is needed.
fn main() {
    let mut total: i64 = 0;
    for depth in 4i32..=20 {
        let iters: i64 = 1 << (20 - depth + 4);
        let nodes: i64 = (1 << (depth + 1)) - 1;
        total += iters * nodes;
    }
    println!("{}", total);
}
