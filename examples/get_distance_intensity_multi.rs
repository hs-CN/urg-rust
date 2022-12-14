fn main() {
    let mut urg = urg_rust::Urg::open("192.168.0.10".parse().unwrap(), 10940).unwrap();
    println!("start capture");
    urg.start_capture().unwrap();
    println!("{:?}", urg.get_status_info().unwrap());

    let payload = urg.get_distance_intensity_multi(0, 1080, 0, 0, 10).unwrap();
    for res in payload {
        match res {
            Ok(payload) => {
                println!("{}", payload.time_stamp);
                println!("{:?}", payload.distance);
                println!("{:?}", payload.intensity);
            }
            Err(err) => println!("{}", err),
        }
    }

    println!("stop capture");
    urg.stop_capture().unwrap();
    println!("{:?}", urg.get_status_info().unwrap());
}
