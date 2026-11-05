// std HashMap baseline: 1M inserts + 1M lookups.
use std::collections::HashMap;

fn main() {
    let size = 2_097_152usize;
    let n = 1_000_000i64;
    let mut map: HashMap<i64, i64> = HashMap::with_capacity(size);
    for i in 1..=n {
        map.insert(i, i * 3);
    }
    let mut total = 0i64;
    for i in 1..=n {
        total += map[&i];
    }
    println!("{}", total);
}
