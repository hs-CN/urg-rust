use urg_rust;
fn main() {
    let mut urg = urg_rust::Urg::open("192.168.0.10", 10940).unwrap();
    println!("{:?}", urg.get_status_info());
}
