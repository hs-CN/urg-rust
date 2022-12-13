fn main() {
    let mut urg = urg_rust::Urg::open("192.168.0.10".parse().unwrap(), 10940).unwrap();
    println!("start capture");
    urg.start_capture().unwrap();
    println!("{:?}", urg.get_status_info().unwrap());

    let payload = urg.get_distance_multi(0, 1080, 0, 0, 10).unwrap();
    for res in payload {
        if let Ok(res) = res {
            println!("{}", res.time_stamp);
            println!("{:?}", res.distance);
        }
    }

    println!("stop capture");
    urg.stop_capture().unwrap();
    println!("{:?}", urg.get_status_info().unwrap());
}
