use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::{fs, net::IpAddr, num::NonZeroU32, path::Path};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
struct Config {
    laser_ip_address: IpAddr,
    laser_port: u16,
    target_plc_ip_address: IpAddr,
    target_plc_ip_port: u16,
    target_plc_modbus_address: u32,
    near_mm: u32,
    far_mm: u32,
    left_deg: u32,
    right_deg: u32,
    min_scan_point: u32,
    min_width_mm: u32,
    max_width_mm: u32,
    scan_count_per_compute: u32,
    line_fit_error: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            laser_ip_address: IpAddr::from([192, 168, 0, 10]),
            laser_port: 10940,
            target_plc_ip_address: IpAddr::from([192, 168, 1, 21]),
            target_plc_ip_port: 502,
            target_plc_modbus_address: 14,
            near_mm: 550,
            far_mm: 650,
            left_deg: 20,
            right_deg: 20,
            min_scan_point: 10,
            min_width_mm: 50,
            max_width_mm: 200,
            scan_count_per_compute: 10,
            line_fit_error: 0.01,
        }
    }
}

fn load_config() -> Config {
    let config_file = Path::new("measure_width.toml");
    let err_msg: String;
    if config_file.exists() {
        match fs::read_to_string(config_file) {
            Ok(str) => match toml::from_str(&str) {
                Ok(config) => return config,
                Err(err) => err_msg = format!("deserialize config {} failed. {}", str, err),
            },
            Err(err) => err_msg = format!("read config file failed. {}", err),
        }
    } else {
        err_msg = format!("config file \"{}\" not found!", config_file.display(),);
    }

    let default_config = Config::default();
    warn!("{}, use default value. {:?}", err_msg, default_config);
    match toml::to_string(&default_config) {
        Ok(str) => {
            if let Err(err) = fs::write(config_file, str) {
                warn!("save default config failed. {}", err)
            }
        }
        Err(err) => warn!("serialize default config failed. {}", err),
    }
    default_config
}

fn distance_avg(data: Vec<(u32, Vec<u32>)>) -> Vec<u32> {
    let count = data.len() as u32;
    let arr_len = data[0].1.len();
    let mut res = vec![0; arr_len];
    for (_, d) in data {
        for i in 0..arr_len {
            res[i] += d[i];
        }
    }
    for i in 0..arr_len {
        res[i] = res[i] / count;
    }
    res
}

fn distance_filter(
    distance: &Vec<u32>,
    near: u32,
    far: u32,
    min_scan_point: u32,
) -> Vec<(u32, u32, u32, u32, u32)> {
    let mut in_range = Vec::new();
    let mut start_index = 0;
    let mut end_index = 0;
    let mut max_d = u32::MIN;
    let mut min_d = u32::MAX;
    let mut sum_d = 0;
    let mut is_in_range = false;
    for d in distance.iter() {
        let d = *d;
        if is_in_range {
            if d < near || d > far {
                is_in_range = false;
                if end_index - start_index > min_scan_point {
                    in_range.push((
                        start_index,
                        end_index - 1,
                        max_d,
                        min_d,
                        sum_d / (end_index - start_index),
                    ));
                }
                start_index = end_index + 1;
                sum_d = 0;
                max_d = u32::MIN;
                min_d = u32::MAX;
            } else {
                sum_d += d;
                if d > max_d {
                    max_d = d;
                }
                if d < min_d {
                    min_d = d;
                }
            }
            end_index += 1;
        } else {
            if d < near || d > far {
                start_index += 1;
            } else {
                is_in_range = true;
                sum_d += d;
                if d > max_d {
                    max_d = d;
                }
                if d < min_d {
                    min_d = d;
                }
            }
            end_index += 1;
        }
    }
    in_range
}

fn width_compute(distance: &Vec<u32>, start_index: usize, end_index: usize) -> u32 {
    let ax = distance[start_index as usize] as f32;

    let theta = (end_index - start_index) as f32 * 0.25;
    let bx = distance[end_index - 1] as f32 * theta.to_radians().cos();
    let by = distance[end_index - 1] as f32 * theta.to_radians().sin();

    ((bx - ax) * (bx - ax) + by * by).sqrt() as u32
}

fn main() {
    simple_logger::SimpleLogger::new()
        .with_local_timestamps()
        .init()
        .expect("init logger error");
    let config_file = load_config();

    let mut urg =
        urg_rust::Urg::open(config_file.laser_ip_address, config_file.laser_port).expect(&format!(
            "open laser {}:{} failed.",
            config_file.laser_ip_address, config_file.laser_port
        ));

    info!("urg paramerers: {:?}", urg);
    info!(
        "urg status: {:?}",
        urg.get_status_info().expect("urg get_status_info failed.")
    );

    let start_step =
        urg.front_dir_step - ((config_file.right_deg as f32) / urg.angular_resolution_deg) as u32;
    let end_step =
        urg.front_dir_step + ((config_file.left_deg as f32) / urg.angular_resolution_deg) as u32;

    urg.start_capture().expect("urg start_capture failed.");
    let scan_count =
        NonZeroU32::new(config_file.scan_count_per_compute).unwrap_or(NonZeroU32::new(10).unwrap());
    info!(
        "urg start capture [{},{}] with scan_count_per_compute:{}",
        start_step, end_step, scan_count
    );
    loop {
        let data = match urg.get_distance_multi(start_step, end_step, 0, 0, scan_count) {
            Ok(data) => data,
            Err(err) => {
                error!("urg get_distance failed.{}", err);
                break;
            }
        };
        let time_stamp = data[data.len() - 1].0;
        let distance = distance_avg(data);
        let in_range = distance_filter(
            &distance,
            config_file.near_mm,
            config_file.far_mm,
            config_file.min_scan_point,
        );
        let mut msg = String::new();
        let mut width_arr = Vec::new();
        for (start_index, end_index, max_d, min_d, avg_d) in &in_range {
            let width = width_compute(&distance, *start_index as usize, *end_index as usize);
            msg = msg
                + &format!(
                    " [{},{}] ({},{},{}) {}mm;",
                    start_step + start_index,
                    start_step + end_index,
                    max_d,
                    min_d,
                    avg_d,
                    width
                );
            if width > config_file.min_width_mm && width < config_file.max_width_mm {
                width_arr.push(width);
            }
        }
        if width_arr.len() == 0 {
            warn!(
                "time_stamp:{} capture [{},{}] not found",
                time_stamp, start_step, end_step
            );
        } else if width_arr.len() > 1 {
            warn!(
                "time_stamp:{} capture [{},{}] found more then 1.{}",
                time_stamp, start_step, end_step, msg
            );
        } else {
            info!(
                "time_stamp:{} capture [{},{}] found{} use:{}mm",
                time_stamp, start_step, end_step, msg, width_arr[0]
            )
        }
    }
    urg.stop_capture().expect("urg stop_capture failed");
}
