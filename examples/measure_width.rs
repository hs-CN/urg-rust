use log::{error, info, warn};
use modbus::Client;
use ndarray::prelude::*;
use ndarray_linalg::*;
use serde::{Deserialize, Serialize};
use std::{fs, net::IpAddr, num::NonZeroU32, path::Path, sync::atomic::AtomicBool};

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Config {
    laser_ip_address: IpAddr,
    laser_port: u16,
    target_plc_ip_address: String,
    target_plc_modbus_address: u32,
    enable_write_to_plc: bool,
    near_mm: u32,
    far_mm: u32,
    fov_deg: u32,
    min_scan_point: u32,
    scan_count_per_compute: u32,
    min_distance_to_fit_line_mm: u32,
    min_width_mm: u32,
    max_width_mm: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            laser_ip_address: IpAddr::from([192, 168, 0, 10]),
            laser_port: 10940,
            target_plc_ip_address: String::from("127.0.0.1"),
            target_plc_modbus_address: 0,
            enable_write_to_plc: false,
            near_mm: 300,
            far_mm: 600,
            fov_deg: 30,
            min_scan_point: 10,
            scan_count_per_compute: 40,
            min_distance_to_fit_line_mm: 45,
            min_width_mm: 80,
            max_width_mm: 300,
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

fn distance_avg(data: Vec<(u32, Vec<u32>)>) -> Vec<f32> {
    let count = data.len() as f32;
    let arr_len = data[0].1.len();
    let mut res = vec![0.0; arr_len];
    for (_, d) in data {
        for i in 0..arr_len {
            res[i] += d[i] as f32;
        }
    }
    for i in 0..arr_len {
        res[i] = res[i] / count;
    }
    res
}

fn distance_filter(
    distance: &Vec<f32>,
    near: f32,
    far: f32,
    min_scan_point: u32,
) -> Vec<(u32, u32, u32, u32, u32)> {
    let mut in_range = Vec::new();
    let mut start_index = 0;
    let mut end_index = 0;
    let mut max_d = f32::MIN;
    let mut min_d = f32::MAX;
    let mut sum_d = 0.0;
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
                        max_d as u32,
                        min_d as u32,
                        sum_d as u32 / (end_index - start_index),
                    ));
                }
                start_index = end_index + 1;
                sum_d = 0.0;
                max_d = f32::MIN;
                min_d = f32::MAX;
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

fn compute_width(
    distance: &Vec<f32>,
    start_index: usize,
    end_index: usize,
    anuglar_resolution_rad: f32,
    min_distance_from_fit_line: u32,
) -> f32 {
    let offset = 90f32.to_radians();

    let mut x_arr = Vec::new();
    let mut y_arr = Vec::new();
    let mut index = 0;
    for d in &distance[start_index..=end_index] {
        let theta = index as f32 * anuglar_resolution_rad + offset;
        x_arr.push(*d as f32 * theta.cos());
        y_arr.push(*d as f32 * theta.sin());
        index += 1;
    }
    let head = (x_arr.len() as f32 * 0.2) as usize;
    let tail = x_arr.len() - head;
    let (a, b) = line_fit_ols(&x_arr[head..tail], &y_arr[head..tail]);
    info!("ols fit line ({},{})", a, b);

    let mut head_index = 0;
    for i in 0..x_arr.len() {
        let diff = (x_arr[i] * a - y_arr[i] + b).abs() / (a * a + 1.0).sqrt();
        if diff < min_distance_from_fit_line as f32 {
            head_index = i;
            break;
        }
    }
    let mut tail_index = 0;
    for i in 0..x_arr.len() {
        let i = x_arr.len() - i - 1;
        let diff = (x_arr[i] * a - y_arr[i] + b).abs() / (a * a + 1.0).sqrt();
        if diff < min_distance_from_fit_line as f32 {
            tail_index = i;
            break;
        }
    }

    let a_theta = head_index as f32 * anuglar_resolution_rad + offset;
    let b_theta = tail_index as f32 * anuglar_resolution_rad + offset;
    info!(
        "[{},{}] cut [{},{}]",
        0,
        end_index - start_index,
        head_index,
        tail_index,
    );

    let ax = b / (a_theta.tan() - a);
    let ay = b * a_theta.tan() / (a_theta.tan() - a);
    let bx = b / (b_theta.tan() - a);
    let by = b * b_theta.tan() / (b_theta.tan() - a);
    ((bx - ax) * (bx - ax) + (by - ay) * (by - ay)).sqrt()
}

pub fn line_fit_ols(x: &[f32], y: &[f32]) -> (f32, f32) {
    assert_eq!(x.len(), y.len());
    let x = hstack(&[ndarray::aview1(x), aview1(&vec![1.0; x.len()])]).unwrap();
    let y = aview1(y);
    let x_t = x.t();
    let a = &x_t.dot(&x);
    let b = &x_t.dot(&y);
    let factor = a.solve(b).unwrap().to_vec();
    (factor[0], factor[1])
}

static TERMINAL_SIGNAL: AtomicBool = AtomicBool::new(false);

fn main() {
    simple_logger::SimpleLogger::new()
        .with_local_timestamps()
        .init()
        .expect("init logger error");
    ctrlc::set_handler(|| TERMINAL_SIGNAL.store(true, std::sync::atomic::Ordering::Relaxed))
        .expect("Error setting Ctrl-C handler");
    let config_file = load_config();

    let mut modbus_client = if config_file.enable_write_to_plc {
        Some(
            modbus::tcp::Transport::new(&config_file.target_plc_ip_address).expect(&format!(
                "connect to modbus server {} failed",
                config_file.target_plc_ip_address
            )),
        )
    } else {
        None
    };

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

    let start_step = urg.front_dir_step
        - ((config_file.fov_deg as f32 * 0.5) / urg.angular_resolution_deg) as u32;
    let end_step = urg.front_dir_step
        + ((config_file.fov_deg as f32 * 0.5) / urg.angular_resolution_deg) as u32;

    urg.start_capture().expect("urg start_capture failed.");
    let scan_count =
        NonZeroU32::new(config_file.scan_count_per_compute).unwrap_or(NonZeroU32::new(10).unwrap());
    info!(
        "urg start capture [{},{}] with scan_count_per_compute:{}",
        start_step, end_step, scan_count
    );
    loop {
        if TERMINAL_SIGNAL.load(std::sync::atomic::Ordering::Relaxed) {
            info!("recv Ctrl + C, waiting for urg close.");
            break;
        }
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
            config_file.near_mm as f32,
            config_file.far_mm as f32,
            config_file.min_scan_point,
        );
        let mut msg = String::new();
        let mut width_arr = Vec::new();
        for (start_index, end_index, max_d, min_d, avg_d) in &in_range {
            let width = compute_width(
                &distance,
                *start_index as usize,
                *end_index as usize,
                urg.angular_resolution_deg.to_radians(),
                config_file.min_distance_to_fit_line_mm,
            );
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
            if width > config_file.min_width_mm as f32 && width < config_file.max_width_mm as f32 {
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
            );
            if let Some(ref mut modbus_client) = modbus_client {
                let value = width_arr[0].round() as u16;
                info!(
                    "write width {}mm to holding register {}",
                    value, config_file.target_plc_modbus_address
                );
                modbus_client
                    .write_single_register(config_file.target_plc_modbus_address as u16, value)
                    .unwrap();
            }
        }
    }
    urg.stop_capture().expect("urg stop_capture failed");
    info!(
        "urg status: {:?}",
        urg.get_status_info().expect("urg get_status_info failed.")
    );
}
