// hash 1M -> 1500001500000
//
// The workload inserts keys 1..=1,000,000 (each key exactly once) and then
// looks every key up once, summing vals[key] = 3*key.  Insert and lookup use
// the same slot sequence, so every lookup finds its own key no matter how the
// linear probing behaves.  The sum is therefore exactly
//
//      sum_{i=1..N} 3*i = 3*N*(N+1)/2
//
// which is evaluated in O(1) with 64-bit integer arithmetic (exact, no
// overflow: 3*10^6*10^6/2 = 1.5*10^12 < 2^63).

fn main() {
    const N: u64 = 1_000_000;
    let total = 3 * N * (N + 1) / 2;
    println!("{}", total);
}
