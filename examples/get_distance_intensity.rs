use urg_rust;
fn main() {
    let mut urg = urg_rust::Urg::open("192.168.0.10".parse().unwrap(), 10940).unwrap();
    println!("start capture");
    urg.start_capture().unwrap();
    println!("{:?}", urg.get_status_info().unwrap());

    let (time_stamp, distance, intensity) = urg.get_distance_intensity(0, 1080, 0).unwrap();
    println!("{}", time_stamp);
    println!("{:?}", distance);
    println!("{:?}", intensity);

    println!("stop capture");
    urg.stop_capture().unwrap();
    println!("{:?}", urg.get_status_info().unwrap());
}
