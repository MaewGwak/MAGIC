// Sieve, identical work: mark only up to sqrt(N), then count all primes.
// Expected: 664579.
const N: usize = 100_000_000;

fn main() {
    let mut s = vec![1u8; N];

    let mut i = 2usize;
    while i * i < N {
        if s[i] == 1 {
            let mut j = i * i;
            while j < N {
                s[j] = 0;
                j += i;
            }
        }
        i += 1;
    }

    let mut count: u64 = 0;
    for k in 2..N {
        if s[k] == 1 {
            count += 1;
        }
    }
    println!("{}", count);
}
