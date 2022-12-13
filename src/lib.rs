use anyhow::bail;
use bstr::{BString, ByteSlice};
use std::{
    io::{self, BufRead, BufReader, BufWriter, Write},
    net::{IpAddr, TcpStream},
    sync::Arc,
};

#[derive(Debug)]
pub struct StatusInfo {
    pub sensor_model: BString,
    pub laser_status: BString,
    pub scanning_speed_rpm: u32,
    pub measurement_mode: BString,
    pub communication_speed: BString,
    pub time_stamp: u32,
    pub sensor_status: BString,
}

#[derive(Debug)]
pub struct VersionInfo {
    pub vendor_info: BString,
    pub product_info: BString,
    pub firmware_version: BString,
    pub protocol_version: BString,
    pub serial_number: BString,
}

#[derive(Debug)]
pub struct SensorParams {
    pub sensor_model: BString,
    pub min_distance_mm: u32,
    pub max_distance_mm: u32,
    pub angular_resolution_deg: f32,
    pub start_step: u32,
    pub end_step: u32,
    pub front_dir_step: u32,
    pub std_scan_speed_rpm: u32,
}

#[derive(Debug)]
pub struct UrgPayload {
    pub time_stamp: u32,
    pub distance: Vec<u32>,
    pub intensity: Vec<u32>,
}

pub struct UrgPayloadIter {
    stream: Arc<TcpStream>,
    count: Option<u32>,
    cmd: String,
    has_intensity: bool,
    buffer: Vec<u8>,
}

impl Iterator for UrgPayloadIter {
    type Item = anyhow::Result<UrgPayload>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(count) = self.count {
            if count == 0 {
                return None;
            }
        }

        let mut reader = BufReader::new(self.stream.as_ref());
        if let Some(count) = self.count {
            self.count = Some(count - 1);
            let cmd = format!("{}{:0>2}", &self.cmd[..self.cmd.len() - 2], count - 1);
            if let Err(err) = check_send_cmd_response(&mut reader, &mut self.buffer, &cmd, "99") {
                return Some(Err(err));
            }
        } else if let Err(err) =
            check_send_cmd_response(&mut reader, &mut self.buffer, &self.cmd, "99")
        {
            return Some(Err(err));
        }

        match get_raw_data(&mut reader, &mut self.buffer) {
            Ok((time_stamp, raw_data)) => {
                if self.has_intensity {
                    let mut distance = Vec::new();
                    let mut intensity = Vec::new();
                    for bytes in raw_data.chunks_exact(6) {
                        distance.push(decode(&bytes[0..3]));
                        intensity.push(decode(&bytes[3..6]));
                    }
                    Some(Ok(UrgPayload {
                        time_stamp,
                        distance,
                        intensity,
                    }))
                } else {
                    let mut distance = Vec::new();
                    for bytes in raw_data.chunks_exact(3) {
                        distance.push(decode(&bytes[0..3]));
                    }
                    Some(Ok(UrgPayload {
                        time_stamp,
                        distance,
                        intensity: Vec::new(),
                    }))
                }
            }
            Err(err) => Some(Err(err)),
        }
    }
}

#[derive(Debug)]
pub struct Urg {
    stream: Arc<TcpStream>,
    pub is_capturing: bool,
    pub ip_address: IpAddr,
    pub port: u16,
}

impl Urg {
    pub fn open(ip_address: IpAddr, port: u16) -> io::Result<Self> {
        let stream = Arc::new(TcpStream::connect((ip_address, port))?);
        Ok(Self {
            stream,
            is_capturing: false,
            ip_address,
            port,
        })
    }

    pub fn get_version_info(&self) -> anyhow::Result<VersionInfo> {
        let reader = self.stream.clone();
        let mut reader = BufReader::new(reader.as_ref());
        let writer = self.stream.clone();
        let mut writer = BufWriter::new(writer.as_ref());
        let mut buffer = Vec::new();

        Self::send_cmd(&mut reader, &mut writer, &mut buffer, "VV", "00")?;
        let vendor_info = Self::recv_b_string_sub(&mut reader, &mut buffer)?;
        let product_info = Self::recv_b_string_sub(&mut reader, &mut buffer)?;
        let firmware_version = Self::recv_b_string_sub(&mut reader, &mut buffer)?;
        let protocol_version = Self::recv_b_string_sub(&mut reader, &mut buffer)?;
        let serial_number = Self::recv_b_string_sub(&mut reader, &mut buffer)?;
        _ = recv_data(&mut reader, &mut buffer)?;

        Ok(VersionInfo {
            vendor_info,
            product_info,
            firmware_version,
            protocol_version,
            serial_number,
        })
    }

    pub fn get_sensor_params(&self) -> anyhow::Result<SensorParams> {
        let reader = self.stream.clone();
        let mut reader = BufReader::new(reader.as_ref());
        let writer = self.stream.clone();
        let mut writer = BufWriter::new(writer.as_ref());
        let mut buffer = Vec::new();

        Self::send_cmd(&mut reader, &mut writer, &mut buffer, "PP", "00")?;
        let sensor_model = Self::recv_b_string_sub(&mut reader, &mut buffer)?;
        let min_distance_mm = Self::recv_b_string_u32(&mut reader, &mut buffer)?;
        let max_distance_mm = Self::recv_b_string_u32(&mut reader, &mut buffer)?;
        let angular_area = Self::recv_b_string_u32(&mut reader, &mut buffer)?;
        let angular_resolution_deg = 360.0 / angular_area as f32;
        let start_step = Self::recv_b_string_u32(&mut reader, &mut buffer)?;
        let end_step = Self::recv_b_string_u32(&mut reader, &mut buffer)?;
        let front_dir_step = Self::recv_b_string_u32(&mut reader, &mut buffer)?;
        let std_scan_speed_rpm = Self::recv_b_string_u32(&mut reader, &mut buffer)?;
        // let scan_direction = Self::recv_b_string(&mut reader, &mut buffer)?;
        _ = recv_data(&mut reader, &mut buffer)?;

        Ok(SensorParams {
            sensor_model,
            min_distance_mm,
            max_distance_mm,
            angular_resolution_deg,
            start_step,
            end_step,
            front_dir_step,
            std_scan_speed_rpm,
        })
    }

    pub fn get_status_info(&self) -> anyhow::Result<StatusInfo> {
        let reader = self.stream.clone();
        let mut reader = BufReader::new(reader.as_ref());
        let writer = self.stream.clone();
        let mut writer = BufWriter::new(writer.as_ref());
        let mut buffer = Vec::new();

        Self::send_cmd(&mut reader, &mut writer, &mut buffer, "II", "00")?;
        let sensor_model = Self::recv_b_string_sub(&mut reader, &mut buffer)?;
        let laser_status = Self::recv_b_string_sub(&mut reader, &mut buffer)?;
        let scanning_speed_rpm = Self::recv_b_string_u32(&mut reader, &mut buffer)?;
        let measurement_mode = Self::recv_b_string_sub(&mut reader, &mut buffer)?;
        let communication_speed = Self::recv_b_string_sub(&mut reader, &mut buffer)?;
        let time_stamp = decode(&Self::recv_b_string_sub(&mut reader, &mut buffer)?);
        let sensor_status = Self::recv_b_string_sub(&mut reader, &mut buffer)?;
        _ = recv_data(&mut reader, &mut buffer)?;

        Ok(StatusInfo {
            sensor_model,
            laser_status,
            scanning_speed_rpm,
            measurement_mode,
            communication_speed,
            time_stamp,
            sensor_status,
        })
    }

    pub fn start_capture(&mut self) -> anyhow::Result<()> {
        let reader = self.stream.clone();
        let mut reader = BufReader::new(reader.as_ref());
        let writer = self.stream.clone();
        let mut writer = BufWriter::new(writer.as_ref());
        let mut buffer = Vec::new();

        Self::send_cmd(&mut reader, &mut writer, &mut buffer, "BM", "00")?;
        _ = recv_data(&mut reader, &mut buffer)?;
        self.is_capturing = true;

        Ok(())
    }

    pub fn stop_capture(&mut self) -> anyhow::Result<()> {
        let reader = self.stream.clone();
        let mut reader = BufReader::new(reader.as_ref());
        let writer = self.stream.clone();
        let mut writer = BufWriter::new(writer.as_ref());
        let mut buffer = Vec::new();

        Self::send_cmd(&mut reader, &mut writer, &mut buffer, "QT", "00")?;
        _ = recv_data(&mut reader, &mut buffer)?;
        self.is_capturing = false;

        Ok(())
    }

    pub fn reboot(self) -> anyhow::Result<()> {
        let reader = self.stream.clone();
        let mut reader = BufReader::new(reader.as_ref());
        let writer = self.stream;
        let mut writer = BufWriter::new(writer.as_ref());
        let mut buffer = Vec::new();

        Self::send_cmd(&mut reader, &mut writer, &mut buffer, "RB", "01")?;
        _ = recv_data(&mut reader, &mut buffer)?;
        Self::send_cmd(&mut reader, &mut writer, &mut buffer, "RB", "00")?;
        _ = recv_data(&mut reader, &mut buffer)?;

        Ok(())
    }

    pub fn get_distance(
        &self,
        start_step: u32,
        end_step: u32,
        cluster_count: u32,
    ) -> anyhow::Result<UrgPayload> {
        let reader = self.stream.clone();
        let mut reader = BufReader::new(reader.as_ref());
        let writer = self.stream.clone();
        let mut writer = BufWriter::new(writer.as_ref());
        let mut buffer = Vec::new();

        let cmd = format!("GD{:0>4}{:0>4}{:0>2}", start_step, end_step, cluster_count);
        Self::send_cmd(&mut reader, &mut writer, &mut buffer, &cmd, "00")?;
        let (time_stamp, raw_data) = get_raw_data(&mut reader, &mut buffer)?;
        let mut distance = Vec::new();
        for bytes in raw_data.chunks_exact(3) {
            distance.push(decode(bytes));
        }

        Ok(UrgPayload {
            time_stamp,
            distance,
            intensity: Vec::new(),
        })
    }

    pub fn get_distance_multi(
        &self,
        start_step: u32,
        end_step: u32,
        cluster_count: u32,
        scan_skip_count: u32,
        num_of_scan: u32,
    ) -> anyhow::Result<UrgPayloadIter> {
        let reader = self.stream.clone();
        let mut reader = BufReader::new(reader.as_ref());
        let writer = self.stream.clone();
        let mut writer = BufWriter::new(writer.as_ref());
        let mut buffer = Vec::new();

        let cmd = format!(
            "MD{:0>4}{:0>4}{:0>2}{:0>1}{:0>2}",
            start_step, end_step, cluster_count, scan_skip_count, num_of_scan
        );
        Self::send_cmd(&mut reader, &mut writer, &mut buffer, &cmd, "00")?;
        _ = recv_data(&mut reader, &mut buffer)?;

        let count = if num_of_scan == 0 {
            None
        } else {
            Some(num_of_scan)
        };

        Ok(UrgPayloadIter {
            stream: self.stream.clone(),
            count,
            cmd,
            has_intensity: false,
            buffer,
        })
    }

    pub fn get_distance_intensity(
        &self,
        start_step: u32,
        end_step: u32,
        cluster_count: u32,
    ) -> anyhow::Result<UrgPayload> {
        let reader = self.stream.clone();
        let mut reader = BufReader::new(reader.as_ref());
        let writer = self.stream.clone();
        let mut writer = BufWriter::new(writer.as_ref());
        let mut buffer = Vec::new();

        let cmd = format!("GE{:0>4}{:0>4}{:0>2}", start_step, end_step, cluster_count);
        Self::send_cmd(&mut reader, &mut writer, &mut buffer, &cmd, "00")?;
        let (time_stamp, raw_data) = get_raw_data(&mut reader, &mut buffer)?;
        let mut distance = Vec::new();
        let mut intensity = Vec::new();
        for bytes in raw_data.chunks_exact(6) {
            distance.push(decode(&bytes[0..3]));
            intensity.push(decode(&bytes[3..6]));
        }
        Ok(UrgPayload {
            time_stamp,
            distance,
            intensity,
        })
    }

    pub fn get_distance_intensity_multi(
        &self,
        start_step: u32,
        end_step: u32,
        cluster_count: u32,
        scan_skip_count: u32,
        num_of_scan: u32,
    ) -> anyhow::Result<UrgPayloadIter> {
        let reader = self.stream.clone();
        let mut reader = BufReader::new(reader.as_ref());
        let writer = self.stream.clone();
        let mut writer = BufWriter::new(writer.as_ref());
        let mut buffer = Vec::new();

        let cmd = format!(
            "ME{:0>4}{:0>4}{:0>2}{:0>1}{:0>2}",
            start_step, end_step, cluster_count, scan_skip_count, num_of_scan
        );
        Self::send_cmd(&mut reader, &mut writer, &mut buffer, &cmd, "00")?;
        _ = recv_data(&mut reader, &mut buffer)?;

        let count = if num_of_scan == 0 {
            None
        } else {
            Some(num_of_scan)
        };

        Ok(UrgPayloadIter {
            stream: self.stream.clone(),
            count,
            cmd,
            has_intensity: true,
            buffer,
        })
    }

    fn recv_b_string(reader: &mut impl BufRead, buffer: &mut Vec<u8>) -> anyhow::Result<BString> {
        let n = recv_data(reader, buffer)?;
        if n < 2 {
            bail!("can not convert to BString. recv bytes len:{n}");
        }
        Ok(BString::new(buffer[..n - 2].to_vec()))
    }

    fn recv_b_string_sub(
        reader: &mut impl BufRead,
        buffer: &mut Vec<u8>,
    ) -> anyhow::Result<BString> {
        let str = Self::recv_b_string(reader, buffer)?;
        let len = str.len();
        if len < 6 {
            bail!("can not sub BString. BString:{str} length: {len}")
        }
        Ok(BString::new(str[5..len - 1].to_vec()))
    }

    fn recv_b_string_u32(reader: &mut impl BufRead, buffer: &mut Vec<u8>) -> anyhow::Result<u32> {
        let digit_str = Self::recv_b_string_sub(reader, buffer)?;
        Ok(digit_str.to_str()?.parse()?)
    }

    fn send_cmd(
        reader: &mut impl BufRead,
        writer: &mut impl Write,
        buffer: &mut Vec<u8>,
        cmd: &str,
        ok_status: &str,
    ) -> anyhow::Result<()> {
        writer.write_all(cmd.as_bytes())?;
        writer.write_all(&[b'\n'])?;
        writer.flush()?;
        check_send_cmd_response(reader, buffer, cmd, ok_status)
    }
}

fn decode(raw: &[u8]) -> u32 {
    let mut res = 0;
    for byte in raw {
        res <<= 6;
        res += ((byte - 0x30) & 0b00111111) as u32;
    }
    res
}

fn get_raw_data(reader: &mut impl BufRead, buffer: &mut Vec<u8>) -> anyhow::Result<(u32, Vec<u8>)> {
    let n = recv_data(reader, buffer)?;
    if n != 6 {
        bail!(
            "get_distance failed. recv wrong timestamp data {:?}",
            buffer
        );
    }
    let time_stamp = decode(&buffer[..4]);

    let mut raw_data: Vec<u8> = Vec::new();
    loop {
        let n = recv_data(reader, buffer)?;
        if n == 1 {
            break;
        } else {
            raw_data.extend_from_slice(&buffer[..n - 2]);
        }
    }
    Ok((time_stamp, raw_data))
}

#[inline]
fn recv_data(reader: &mut impl BufRead, buffer: &mut Vec<u8>) -> io::Result<usize> {
    buffer.clear();
    reader.read_until(b'\n', buffer)
}

fn check_send_cmd_response(
    reader: &mut impl BufRead,
    buffer: &mut Vec<u8>,
    cmd: &str,
    ok_status: &str,
) -> anyhow::Result<()> {
    let n = recv_data(reader, buffer)?;
    if &buffer[..n - 1] != cmd.as_bytes() {
        bail!(
            "send cmd: {} failed. recv {} != {}",
            cmd,
            &buffer[..n - 1].as_bstr(),
            cmd
        );
    }
    let n = recv_data(reader, buffer)?;
    if &buffer[..n - 2] != ok_status.as_bytes() {
        bail!(
            "send cmd: {} failed, status error {} != {}",
            cmd,
            ok_status,
            &buffer[..n - 2].as_bstr()
        );
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::decode;

    #[test]
    fn decode_test() {
        let res = decode(b"1Dh");
        assert_eq!(res, 5432);
        let res = decode(&[0x31, 0x44, 0x68]);
        assert_eq!(res, 5432);
    }
}
