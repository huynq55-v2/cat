fn main() {
    let mut v = Vec::new();
    for i in 0..1000 {
        v.push(i);
    }
    println!("==============================");
    println!("Hello from user space!");
    println!("{}", v.len());
    println!("==============================");
}
