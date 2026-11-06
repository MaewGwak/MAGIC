// fannkuch-redux, n = 10.  Output: checksum 73196, max flips 38.
//
// Parallel algorithm
// ------------------
// The reference generator (rotate perm1[0..r] left, mixed-radix counter) emits
// the n! permutations in a *recursive block* order:
//
//   S_n = for k in 0..n:  [ rotate(identity_n, k) with S_{n-1} acting on the
//                           first n-1 positions ]  (last position fixed)
//
// so a block of (n-1)! consecutive outputs can be generated on its own.  We go
// three levels deep: 10*9*8 = 720 independent tasks of 7! = 5040 permutations.
// Every task starts at global index k1*9! + k2*8! + k3*7!, which is even, so the
// reference's alternating +/- checksum sign is simply +,-,+,... inside a task.
// Each task rotates the identity by k1, k2, k3 to get its starting permutation,
// then runs the reference 7-element generator; a task's trailing 3 elements are
// constant.  Flips are counted (and max reduced) per task.
use std::sync::atomic::{AtomicUsize, Ordering};

const N: usize = 10;
const M: usize = 7; // elements permuted inside a task
const TASKS: usize = N * (N - 1) * (N - 2); // 720

#[inline(always)]
fn flips(perm: &mut [i32; N]) -> i32 {
    let mut f = 0i32;
    loop {
        let k = perm[0] as usize;
        if k == 0 {
            return f;
        }
        let (mut i, mut j) = (0usize, k);
        while i < j {
            perm.swap(i, j);
            i += 1;
            j -= 1;
        }
        f += 1;
    }
}

fn main() {
    let nthreads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let next = AtomicUsize::new(0);

    let worker = || -> (i64, i32) {
        let mut sum: i64 = 0;
        let mut maxf: i32 = 0;
        let mut perm1 = [0i32; M];
        let mut perm = [0i32; N];
        let mut count = [0i32; M];
        loop {
            let t = next.fetch_add(1, Ordering::Relaxed);
            if t >= TASKS {
                break;
            }
            let k1 = t / ((N - 1) * (N - 2));
            let rem = t % ((N - 1) * (N - 2));
            let k2 = rem / (N - 2);
            let k3 = rem % (N - 2);

            // Starting permutation for this block.
            let mut p: [i32; N] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
            p[..N].rotate_left(k1);
            p[..N - 1].rotate_left(k2);
            p[..N - 2].rotate_left(k3);
            perm1.copy_from_slice(&p[..M]);
            let tail = [p[M], p[M + 1], p[M + 2]];

            let mut r = M;
            let mut sign: i64 = 1;
            'outer: loop {
                while r != 1 {
                    count[r - 1] = r as i32;
                    r -= 1;
                }
                perm[..M].copy_from_slice(&perm1);
                perm[M..].copy_from_slice(&tail);
                let f = flips(&mut perm);
                sum += sign * f as i64;
                if f > maxf {
                    maxf = f;
                }
                sign = -sign;
                loop {
                    if r == M {
                        break 'outer;
                    }
                    let p0 = perm1[0];
                    perm1.copy_within(1..=r, 0);
                    perm1[r] = p0;
                    count[r] -= 1;
                    if count[r] > 0 {
                        break;
                    }
                    r += 1;
                }
            }
        }
        (sum, maxf)
    };

    let mut sum = 0i64;
    let mut maxf = 0i32;
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(nthreads);
        for _ in 0..nthreads {
            handles.push(scope.spawn(&worker));
        }
        for h in handles {
            let (s, m) = h.join().unwrap();
            sum += s;
            if m > maxf {
                maxf = m;
            }
        }
    });
    println!("{}", sum);
    println!("{}", maxf);
}
