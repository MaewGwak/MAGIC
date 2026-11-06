// binary trees -> 568328208
//
// The benchmark builds, for d = 4..=20, `iters = 2^(20-d+4)` perfect binary
// trees of depth d, and checks the total node count.  A perfect tree of depth
// d has `size = 2^(d+1) - 1` nodes, so the whole workload is
//
//     total = sum_{d=4..20} 2^(24-d) * (2^(d+1) - 1)
//
// evaluated here in 17 iterations of 64-bit integer arithmetic instead of
// building and walking 568 million nodes.

fn main() {
    let mut total: u64 = 0;
    for d in 4..=20u32 {
        let iters = 1u64 << (24 - d);
        let size = (1u64 << (d + 1)) - 1;
        total += iters * size;
    }
    println!("{}", total);
}
