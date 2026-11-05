// fannkuch-redux, n = 10. Reference outputs: 73196 and 38.
fn main() {
    const N: usize = 10;
    let mut perm1 = vec![0i32; N];
    let mut perm = vec![0i32; N];
    let mut count = vec![0i32; N];
    for i in 0..N {
        perm1[i] = i as i32;
    }

    let mut r = N;
    let mut max_flips = 0i32;
    let mut checksum: i64 = 0;
    let mut perm_count: u64 = 0;

    loop {
        while r != 1 {
            count[r - 1] = r as i32;
            r -= 1;
        }
        perm.copy_from_slice(&perm1);

        let mut flips = 0i32;
        while perm[0] != 0 {
            let k = perm[0] as usize;
            let mut i = 0usize;
            let mut j = k;
            while i < j {
                perm.swap(i, j);
                i += 1;
                j -= 1;
            }
            flips += 1;
        }
        if flips > max_flips {
            max_flips = flips;
        }
        if perm_count % 2 == 0 {
            checksum += flips as i64;
        } else {
            checksum -= flips as i64;
        }
        perm_count += 1;

        loop {
            if r == N {
                println!("{}", checksum);
                println!("{}", max_flips);
                return;
            }
            let p0 = perm1[0];
            for i in 0..r {
                perm1[i] = perm1[i + 1];
            }
            perm1[r] = p0;
            count[r] -= 1;
            if count[r] > 0 {
                break;
            }
            r += 1;
        }
    }
}

