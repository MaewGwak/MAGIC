// fannkuch n=10 -> 73196 (checksum), 38 (max flips)
//
// The reference walks all 10! = 3,628,800 permutations with the classic
// mixed-radix odometer.  Its enumeration decomposes exactly into 720
// contiguous blocks of 7! = 5040 permutations: inside a block positions
// 7,8,9 are fixed and positions 0..6 run the same odometer.  A block's first
// global index is q*5040, which is even, so the local permutation parity is
// the global parity and each block can be processed independently.
//
// Block q encodes the three fixed trailing elements in the factorial number
// system: d9 = q/72 full left rotations, d8 = (q/8)%9 rotations of the first
// 9, d7 = q%8 rotations of the first 8.  The 720 blocks are handed to the
// worker threads through an atomic counter (dynamic load balancing).
//
// A permutation is a 128-bit vector of 10 byte values.  Both hot operations
// are single pshufb instructions:
//   * rotating the odometer prefix 0..=r left by one (ROT[r+1]);
//   * reversing the pancake prefix 0..=perm[0] (MASKS[perm[0]]).
// The flip loop is then: extract lane 0, look up the mask, shuffle, repeat.

use std::arch::x86_64::*;
use std::sync::atomic::{AtomicUsize, Ordering};

const NBLOCKS: usize = 720;

// ROT[l][i] = (i+1) % l for i < l, else i: left-rotate the first l bytes.
static ROT: [[u8; 16]; 10] = {
    let mut m = [[0u8; 16]; 10];
    let mut l = 0;
    while l < 10 {
        let mut i = 0;
        while i < 16 {
            m[l][i] = if i < l { ((i + 1) % l) as u8 } else { i as u8 };
            i += 1;
        }
        l += 1;
    }
    m
};

// MASKS[k][i] = k-i for i <= k, else i: reverse the first k+1 bytes.
static MASKS: [[u8; 16]; 10] = {
    let mut m = [[0u8; 16]; 10];
    let mut k = 0;
    while k < 10 {
        let mut i = 0;
        while i < 16 {
            m[k][i] = if i <= k { (k - i) as u8 } else { i as u8 };
            i += 1;
        }
        k += 1;
    }
    m
};

#[inline(always)]
fn rot(a: &mut [u8; 16], len: usize, by: usize) {
    if by == 0 {
        return;
    }
    let mut t = [0u8; 16];
    t[..len].copy_from_slice(&a[..len]);
    for i in 0..len {
        a[i] = t[(i + by) % len];
    }
}

#[inline(always)]
fn flip_count(mut perm: __m128i) -> u32 {
    unsafe {
        let mut flips = 0u32;
        loop {
            let k = (_mm_cvtsi128_si32(perm) & 0xff) as usize;
            if k == 0 {
                return flips;
            }
            let m = _mm_loadu_si128(MASKS[k].as_ptr() as *const __m128i);
            perm = _mm_shuffle_epi8(perm, m);
            flips += 1;
        }
    }
}

fn run_block(q: usize) -> (i64, u32) {
    let mut init = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 0, 0, 0, 0, 0];
    rot(&mut init, 10, q / 72);
    rot(&mut init, 9, (q / 8) % 9);
    rot(&mut init, 8, q % 8);
    let mut perm1 = unsafe { _mm_loadu_si128(init.as_ptr() as *const __m128i) };

    let mut count = [0u8; 8]; // count[1..=7]
    let mut r = 7usize;
    let mut parity = 0u32;
    let mut sum = 0i64;
    let mut mx = 0u32;

    loop {
        while r != 1 {
            count[r - 1] = r as u8;
            r -= 1;
        }

        let f = flip_count(perm1);
        if f > mx {
            mx = f;
        }
        if parity == 0 {
            sum += f as i64;
        } else {
            sum -= f as i64;
        }
        parity ^= 1;

        loop {
            if r == 7 {
                return (sum, mx);
            }
            perm1 = unsafe {
                _mm_shuffle_epi8(perm1, _mm_loadu_si128(ROT[r + 1].as_ptr() as *const __m128i))
            };
            count[r] -= 1;
            if count[r] > 0 {
                break;
            }
            r += 1;
        }
    }
}

fn main() {
    let nthreads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let next = AtomicUsize::new(0);
    let mut checksum = 0i64;
    let mut max_flips = 0u32;

    std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(nthreads);
        for _ in 0..nthreads {
            handles.push(s.spawn(|| {
                let mut sum = 0i64;
                let mut mx = 0u32;
                loop {
                    let q = next.fetch_add(1, Ordering::Relaxed);
                    if q >= NBLOCKS {
                        break;
                    }
                    let (a, b) = run_block(q);
                    sum += a;
                    if b > mx {
                        mx = b;
                    }
                }
                (sum, mx)
            }));
        }
        for h in handles {
            let (a, b) = h.join().unwrap();
            checksum += a;
            if b > max_flips {
                max_flips = b;
            }
        }
    });

    println!("{}", checksum);
    println!("{}", max_flips);
}
