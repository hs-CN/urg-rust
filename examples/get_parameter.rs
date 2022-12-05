use urg_rust;
fn main() {
    let urg = urg_rust::Urg::open("192.168.0.10", 10940).unwrap();
    println!("{:?}", urg);
}
