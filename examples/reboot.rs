fn main() {
    let urg = urg_rust::Urg::open("192.168.0.10".parse().unwrap(), 10940).unwrap();
    println!("{:?}", urg);
    println!("{:?}", urg.get_status_info().unwrap());
    println!("reboot");
    urg.reboot().unwrap();
}
