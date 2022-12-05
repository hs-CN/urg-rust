use urg_rust;
fn main() {
    let mut urg = urg_rust::Urg::open("192.168.0.10", 10940).unwrap();
    println!("start capture");
    urg.start_capture().unwrap();
    println!("{:?}", urg.get_status_info().unwrap());

    let count = std::cell::Cell::new(0);
    urg.get_distance_infinite(0, 1080, 0, 0, |time_stamp, distance| {
        println!("{}", time_stamp);
        println!("{:?}", distance);
        count.set(count.get() + 1);
        count.get() == 11
    })
    .unwrap();

    println!("stop capture");
    urg.stop_capture().unwrap();
    println!("{:?}", urg.get_status_info().unwrap());
}
