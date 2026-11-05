// Goose sieve, N = 10,000,000. Expected: 664579.
import std;

fn main() {
    let N = 100000000;
    var flags: u8[>..] = [];
    flags.resize(N + 1, 1);
    flags[0] = 0;
    flags[1] = 0;

    var i = 2;
    while i * i <= N {
        if flags[i] == 1 {
            var j = i * i;
            while j <= N {
                flags[j] = 0;
                j += i;
            }
        }
        i++;
    }

    var count = 0;
    for k in 2..flags.len {
        if flags[k] == 1 { count++; }
    }
    print(count);
}
