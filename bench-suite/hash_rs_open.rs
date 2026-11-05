// Manual open addressing: same algorithm as MAGIC, for a codegen-fair comparison.
fn main() {
    const SIZE: usize = 2_097_152;
    const N: i64 = 1_000_000;
    let mut keys = vec![0i64; SIZE];
    let mut vals = vec![0i64; SIZE];

    for i in 1..=N {
        let mut slot = ((i * 2_654_435_761) % SIZE as i64) as usize;
        while keys[slot] != 0 {
            slot = (slot + 1) % SIZE;
        }
        keys[slot] = i;
        vals[slot] = i * 3;
    }

    let mut total = 0i64;
    for i in 1..=N {
        let mut slot = ((i * 2_654_435_761) % SIZE as i64) as usize;
        while keys[slot] != i {
            slot = (slot + 1) % SIZE;
        }
        total += vals[slot];
    }
    println!("{}", total);
}
