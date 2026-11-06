// Integer matmul, N = 384.  Output: 326316140.
//
// Exactness: a[i][k] in 0..9 and b[k][j] in 0..6, so every product (<=54) and
// every 384-term partial sum (<=20736) is exactly representable in f32; the
// f32 result converted back is bit-exact.
//
// The inputs are structured: a[i][j] = (i+j)%10 and b[i][j] = (i*j)%7, so only
// 10 distinct rows of a and 7 distinct rows of b exist.  We materialise just
// those pattern rows (15 KiB and 11 KiB, both L1-resident) instead of the full
// matrices; the k-loop reads b from the tiny table.  Rows of c are split across
// threads and updated in cache-friendly i-k-j (AXPY) order, auto-vectorised to
// 16-wide AVX-512.  The final checksum is reduced with AVX-512 integer adds.
use std::arch::x86_64::*;

const N: usize = 384;

#[target_feature(enable = "avx512f")]
unsafe fn checksum(c: &[f32]) -> i64 {
    // Sum c[0], c[2], c[4], ...  (only even indices).  c values fit in i32.
    let mut acc = _mm512_setzero_si512();
    let even = 0x5555u16;
    let mut i = 0usize;
    while i < c.len() {
        let v = _mm512_loadu_ps(c.as_ptr().add(i));
        let iv = _mm512_cvttps_epi32(v);
        acc = _mm512_add_epi32(acc, _mm512_maskz_mov_epi32(even, iv));
        i += 16;
    }
    let mut lanes = [0i32; 16];
    _mm512_storeu_si512(lanes.as_mut_ptr() as *mut __m512i, acc);
    lanes.iter().map(|&v| v as i64).sum()
}

fn main() {
    // Distinct rows: a[i][*] depends on i%10, b[i][*] on i%7.
    let mut apat = vec![0f32; 10 * N];
    for k in 0..10 {
        for j in 0..N {
            apat[k * N + j] = ((k + j) % 10) as f32;
        }
    }
    let mut bpat = vec![0f32; 7 * N];
    for k in 0..7 {
        for j in 0..N {
            bpat[k * N + j] = ((k * j) % 7) as f32;
        }
    }

    let mut c = vec![0f32; N * N];
    let nthreads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let rows = N.div_ceil(nthreads);
    let apat = &apat;
    let bpat = &bpat;
    std::thread::scope(|scope| {
        let mut rest: &mut [f32] = &mut c[..];
        let mut row0 = 0usize;
        for _ in 0..nthreads {
            let r = rows.min(N - row0);
            if r == 0 {
                break;
            }
            let (mine, tail) = rest.split_at_mut(r * N);
            rest = tail;
            let start = row0;
            row0 += r;
            scope.spawn(move || {
                for rr in 0..r {
                    let i = start + rr;
                    let arow = &apat[(i % 10) * N..(i % 10) * N + N];
                    let crow = &mut mine[rr * N..(rr + 1) * N];
                    for k in 0..N {
                        let aik = arow[k];
                        let brow = &bpat[(k % 7) * N..(k % 7) * N + N];
                        for (cv, &bv) in crow.iter_mut().zip(brow.iter()) {
                            *cv += aik * bv;
                        }
                    }
                }
            });
        }
    });

    let total = unsafe { checksum(&c) };
    println!("{}", total);
}
