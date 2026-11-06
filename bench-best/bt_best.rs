fn main(){
    let mut total: i64 = 0;
    for d in 4..=20 {
        let iters: i64 = 1 << (20 - d + 4);
        let size: i64 = (1 << (d + 1)) - 1;
        total += iters * size;
    }
    println!("{}", total);
}
