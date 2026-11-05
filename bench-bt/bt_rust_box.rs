// Rust idiomatic: Box<Node> per node. Expected total: 568328208.
struct Node {
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
}

fn build(depth: i32) -> Node {
    if depth > 0 {
        Node {
            left: Some(Box::new(build(depth - 1))),
            right: Some(Box::new(build(depth - 1))),
        }
    } else {
        Node {
            left: None,
            right: None,
        }
    }
}

fn check(n: &Node) -> i64 {
    let mut c = 1i64;
    if let Some(l) = &n.left {
        c += check(l);
    }
    if let Some(r) = &n.right {
        c += check(r);
    }
    c
}

fn main() {
    let mut total: i64 = 0;
    for d in 4..=20 {
        let iters: usize = 1 << (20 - d + 4);
        for _ in 0..iters {
            let t = build(d);
            total += check(&t);
        }
    }
    println!("{}", total);
}
