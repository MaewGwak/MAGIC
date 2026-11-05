// Rust flat: arena of indices, same shape as MAGIC. Expected total: 568328208.
fn build(left: &mut [i32], right: &mut [i32], depth: i32, next: &mut [i32]) -> i32 {
    let idx = next[0];
    next[0] = idx + 1;
    if depth > 0 {
        left[idx as usize] = build(left, right, depth - 1, next);
        right[idx as usize] = build(left, right, depth - 1, next);
    } else {
        left[idx as usize] = -1;
        right[idx as usize] = -1;
    }
    idx
}

fn check(left: &[i32], right: &[i32], idx: i32) -> i64 {
    if idx < 0 {
        return 0;
    }
    1 + check(left, right, left[idx as usize]) + check(left, right, right[idx as usize])
}

fn main() {
    const MAX: usize = 1 << 21;
    let mut left = vec![0i32; MAX];
    let mut right = vec![0i32; MAX];
    let mut next = vec![0i32; 1];

    let mut total: i64 = 0;
    for d in 4..=20 {
        let iters: usize = 1 << (20 - d + 4);
        for _ in 0..iters {
            next[0] = 0;
            let root = build(&mut left, &mut right, d, &mut next);
            total += check(&left, &right, root);
        }
    }
    println!("{}", total);
}
