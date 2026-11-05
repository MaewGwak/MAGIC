// Open-addressing hash table: 1M inserts + 1M lookups, same as the C.
// Expected: 1500001500000.
import std;

let SIZE = 2097152;
let N = 1000000;

fn main() {
    var keys: i64[>..] = [];
    var vals: i64[>..] = [];
    keys.resize(SIZE, 0);
    vals.resize(SIZE, 0);

    for i in 1..N + 1 {
        var slot = (i * 2654435761) % SIZE;
        while keys[slot] != 0 {
            slot = (slot + 1) % SIZE;
        }
        keys[slot] = i;
        vals[slot] = i * 3;
    }

    var total: i64 = 0;
    for i in 1..N + 1 {
        var slot = (i * 2654435761) % SIZE;
        while keys[slot] != i {
            slot = (slot + 1) % SIZE;
        }
        total += vals[slot];
    }
    print(total);
}
