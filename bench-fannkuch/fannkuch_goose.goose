// Goose fannkuch-redux, n = 10. Reference outputs: 73196 and 38.
import std;

fn main() {
    let n = 10;
    var perm1: i32[10] = [0; 10];
    var perm: i32[10] = [0; 10];
    var count: i64[10] = [0; 10];

    for i in 0..n { perm1[i] = i as i32; }

    var r = n;
    var max_flips = 0;
    var checksum = 0;
    var perm_count = 0;

    loop {
        while r != 1 {
            count[r - 1] = r;
            r--;
        }
        for i in 0..n { perm[i] = perm1[i]; }

        var flips = 0;
        while perm[0] != 0 {
            let k = perm[0];
            var i = 0;
            var j = k;
            while i < j {
                let t = perm[i];
                perm[i] = perm[j];
                perm[j] = t;
                i++;
                j--;
            }
            flips++;
        }
        if flips > max_flips { max_flips = flips; }
        if perm_count % 2 == 0 { checksum += flips; } else { checksum -= flips; }
        perm_count++;

        var advanced = false;
        while !advanced {
            if r == n {
                print(checksum);
                print(max_flips);
                return;
            }
            let p0 = perm1[0];
            for i in 0..r { perm1[i] = perm1[i + 1]; }
            perm1[r] = p0;
            count[r]--;
            if count[r] > 0 { advanced = true; } else { r++; }
        }
    }
}
