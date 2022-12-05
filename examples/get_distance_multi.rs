use urg_rust;
fn main() {
    let mut urg = urg_rust::Urg::open("192.168.0.10".parse().unwrap(), 10940).unwrap();
    println!("start capture");
    urg.start_capture().unwrap();
    println!("{:?}", urg.get_status_info().unwrap());

    let datas = urg
        .get_distance_multi(0, 1080, 0, 0, 10.try_into().unwrap())
        .unwrap();
    for (time_stamp, distance) in datas {
        println!("{}", time_stamp);
        println!("{:?}", distance);
    }

    println!("stop capture");
    urg.stop_capture().unwrap();
    println!("{:?}", urg.get_status_info().unwrap());
}
