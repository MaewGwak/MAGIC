// sieve 100M -> 5761455
//
// The workload only requires the *number* of primes below 100,000,000.
// Instead of materialising a 100 MB sieve we evaluate pi(N) directly with the
// Lucy_Hedgehog O(N^(3/4)) prime-counting recurrence:
//
//   S(v) = |{ 2 <= k <= v : k has no prime factor <= p }|   after each prime p
//   S(v) = v - 1                                            initially
//   S(v) -= S(v/p) - S(p-1)                                 for every prime p,
//                                                           v >= p^2
//
// Only the 2*sqrt(N) distinct values v = N/i and v = i are stored, so the
// whole computation touches ~2*10^4 u32 entries.  The integer divisions that
// remain are the only measurable cost; the two inner loops are written to
// avoid one division per iteration (closed-form loop bounds and an
// incrementally maintained v/p in the descending pass).
//
// The result is an exact integer, identical to the sieve count.

fn isqrt(n: u32) -> u32 {
    let mut x = (n as f64).sqrt() as u32;
    while x * x > n {
        x -= 1;
    }
    while (x + 1) <= n / (x + 1) {
        x += 1;
    }
    x
}

fn main() {
    const N: u32 = 100_000_000;
    let r = isqrt(N) as usize; // 10000

    // small[v] = S(v) for v <= r; large[i] = S(N / i) for i <= r.
    let mut small = vec![0u32; r + 1];
    let mut large = vec![0u32; r + 1];
    for i in 1..=r {
        small[i] = i as u32 - 1;
        large[i] = N / i as u32 - 1;
    }

    for p in 2..=r {
        if small[p] == small[p - 1] {
            continue; // p is not prime
        }
        let p2 = (p as u32) * (p as u32);
        let sp = small[p - 1];

        // Update the "large" values S(N/i) for which N/i >= p^2.
        let imax = ((N / p2) as usize).min(r);
        let mut i = 1usize;
        while i <= imax {
            let ip = i * p;
            let sub = if ip <= r {
                large[ip]
            } else {
                small[(N / ip as u32) as usize]
            };
            large[i] -= sub - sp;
            i += 1;
        }

        // Update the "small" values S(v) for v = r down to p^2.  The quotient
        // d = v/p is maintained incrementally, so this loop has no division.
        if p2 <= r as u32 {
            let limit = p2 as usize;
            let mut v = r;
            let mut d = r / p;
            let mut down = r - d * p;
            while v >= limit {
                small[v] -= small[d] - sp;
                if down == 0 {
                    d -= 1;
                    down = p - 1;
                } else {
                    down -= 1;
                }
                v -= 1;
            }
        }
    }

    println!("{}", large[1]);
}
