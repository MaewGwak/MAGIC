// Sieve of Eratosthenes, 100,000,000.
//
// Algorithm: odd-only, bit-packed segmented sieve.  Every odd number 2k+1 is
// one bit; the bitmap is split into word chunks that are handed to worker
// threads through an atomic counter.  Small primes (<= 61) clear their
// multiples one *word* at a time with a precomputed residue mask (a 64-bit
// AND instead of 64 bit writes), larger primes clear individual bits.
// The final prime count is the popcount of the bitmap plus the prime 2.
//
// Output: 5761455.
use std::sync::atomic::{AtomicUsize, Ordering};

const N: usize = 100_000_000;
const NODD: usize = N / 2; // odd numbers 1,3,...,N-1 -> indices 0..NODD
const NWORDS: usize = NODD / 64; // 781250, exactly 64-bit aligned
const CHUNK: usize = 2048; // 16 KiB (2048 u64 words) per work item

struct SharedPtr(*mut u64);
unsafe impl Send for SharedPtr {}
unsafe impl Sync for SharedPtr {}

fn base_primes(limit: usize) -> Vec<usize> {
    let mut composite = vec![false; limit + 1];
    let mut out = Vec::new();
    for i in 2..=limit {
        if !composite[i] {
            out.push(i);
            let mut j = i * i;
            while j <= limit {
                composite[j] = true;
                j += i;
            }
        }
    }
    out
}

fn main() {
    let mut bits = vec![u64::MAX; NWORDS];
    bits[0] &= !1; // 1 is not prime

    let primes = base_primes(10_000);
    let small: Vec<usize> = primes
        .iter()
        .copied()
        .filter(|&p| p > 2 && p <= 61)
        .collect();

    // masks[p][m] = bits b (0..64) with b % p == m, for p in `small`.
    let mut masks: Vec<(usize, Vec<u64>)> = Vec::with_capacity(small.len());
    for &p in &small {
        let mut tab = vec![0u64; p];
        for b in 0..64 {
            tab[b % p] |= 1u64 << b;
        }
        masks.push((p, tab));
    }
    // (prime, first bit = (p*p-1)/2, residue r = first bit mod p)
    let small_info: Vec<(usize, usize, usize)> = masks
        .iter()
        .map(|(p, _)| {
            let s = (p * p - 1) / 2;
            (*p, s, s % *p)
        })
        .collect();
    let big_info: Vec<(usize, usize)> = primes
        .iter()
        .copied()
        .filter(|&p| p > 61)
        .map(|p| (p, (p * p - 1) / 2))
        .collect();

    let nchunks = NWORDS.div_ceil(CHUNK);
    let next = AtomicUsize::new(0);
    let shared = SharedPtr(bits.as_mut_ptr());
    let nthreads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    let marking = |shared: &SharedPtr,
                   small_info: &[(usize, usize, usize)],
                   masks: &[(usize, Vec<u64>)],
                   big_info: &[(usize, usize)]| {
        loop {
            let c = next.fetch_add(1, Ordering::Relaxed);
            if c >= nchunks {
                break;
            }
            let w0 = c * CHUNK;
            let w1 = (w0 + CHUNK).min(NWORDS);
            let words = unsafe { std::slice::from_raw_parts_mut(shared.0.add(w0), w1 - w0) };

            // Small primes: word-at-a-time pattern clear.
            for (&(p, s, r), (_, tab)) in small_info.iter().zip(masks.iter()) {
                let sw = s / 64;
                let sweep_from;
                if w0 <= sw {
                    // This chunk contains (or starts before) the first multiple.
                    let end = ((sw + 1) * 64).min(w1 * 64);
                    let mut i = s;
                    while i < end {
                        words[(i >> 6) - w0] &= !(1u64 << (i & 63));
                        i += p;
                    }
                    sweep_from = sw + 1;
                } else {
                    sweep_from = w0;
                }
                if sweep_from >= w1 {
                    continue;
                }
                let step = 64 % p;
                let mut m = (r + p - ((sweep_from * 64) % p)) % p;
                for w in sweep_from..w1 {
                    let idx = w - w0;
                    words[idx] &= !tab[m];
                    m += p - step;
                    if m >= p {
                        m -= p;
                    }
                }
            }

            // Larger primes: bit-at-a-time stride p over this chunk.
            for &(p, s) in big_info {
                // First bit >= max(chunk_start, p*p bit) that is a multiple of p.
                let r = s % p;
                let mut i = (w0 * 64).max(s);
                let d = (r + p - (i % p)) % p;
                if d != 0 {
                    i += d;
                }
                let end = w1 * 64;
                while i < end {
                    words[(i >> 6) - w0] &= !(1u64 << (i & 63));
                    i += p;
                }
            }
        }
    };

    std::thread::scope(|scope| {
        for _ in 0..nthreads {
            scope.spawn(|| marking(&shared, &small_info, &masks, &big_info));
        }
    });

    // Count primes: popcount + 1 for the prime 2.
    let mut ones: u64 = bits.iter().map(|w| w.count_ones() as u64).sum();
    ones += 1;
    println!("{}", ones);
}
