// SPDX-License-Identifier: MIT OR Apache-2.0
// Shared parsing logic for Android JNI and iOS C FFI bindings.

use std::collections::HashMap;
use std::io::BufReader;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;

use crate::filesystem;
use crate::gpmf_lens;
use crate::tags_impl::*;
use crate::util::{self, IMUData, VideoMetadata};
use crate::InputOptions;

static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Serialize)]
pub struct GpsPoint {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
    pub speed2d: f64,
    #[serde(rename = "timestamp")]
    pub timestamp_s: f64,
    #[serde(rename = "utcTime")]
    pub utc_time_ms: i64,
    pub fix: i32,
    pub precision: i32,
}

#[derive(Serialize)]
pub struct SensorSampleJson {
    #[serde(rename = "timestampNs")]
    pub timestamp_ns: i64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Serialize)]
pub struct OrientationSampleJson {
    #[serde(rename = "timestampNs")]
    pub timestamp_ns: i64,
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Serialize)]
pub struct VideoInfoJson {
    #[serde(rename = "frameCount")]
    pub frame_count: i32,
    #[serde(rename = "fpsNumerator")]
    pub fps_numerator: i32,
    #[serde(rename = "fpsDenominator")]
    pub fps_denominator: i32,
}

pub struct ParsedData {
    pub camera_type: String,
    pub duration_s: f64,
    pub video_info: Option<(usize, f64)>,
    pub gps_points: Vec<GpsPoint>,
    pub imu_data: Vec<IMUData>,
    pub lens_json: Option<String>,
    pub cori: Vec<(i64, crate::tags_impl::Quaternion<f64>)>,
    pub iori: Vec<(i64, crate::tags_impl::Quaternion<f64>)>,
    pub orientation_combined: Vec<(i64, crate::tags_impl::Quaternion<f64>)>,
}

impl ParsedData {
    pub fn video_info_json(&self) -> Option<VideoInfoJson> {
        self.video_info.map(|(frame_count, fps)| {
            let num = fps.round() as i32;
            let den = if (fps - fps.floor()).abs() < 0.01 {
                1
            } else {
                1001
            };
            VideoInfoJson {
                frame_count: frame_count as i32,
                fps_numerator: num,
                fps_denominator: den,
            }
        })
    }

    pub fn imu_samples_json(
        &self,
        get_vals: fn(&IMUData) -> Option<[f64; 3]>,
    ) -> Vec<SensorSampleJson> {
        self.imu_data
            .iter()
            .filter_map(|d| {
                get_vals(d).map(|v| SensorSampleJson {
                    timestamp_ns: (d.timestamp_ms * 1_000_000.0).round() as i64,
                    x: v[0] as f32,
                    y: v[1] as f32,
                    z: v[2] as f32,
                })
            })
            .collect()
    }

    pub fn orientation_samples_json(
        &self,
        samples: &[(i64, crate::tags_impl::Quaternion<f64>)],
    ) -> Vec<OrientationSampleJson> {
        samples
            .iter()
            .map(|(ts_ns, q)| {
                let e = crate::util::gpmf_orientation_quaternion_export(q);
                OrientationSampleJson {
                    timestamp_ns: *ts_ns,
                    w: e[0],
                    x: e[1],
                    y: e[2],
                    z: e[3],
                }
            })
            .collect()
    }
}

fn parsed_data_map() -> &'static Mutex<HashMap<u64, ParsedData>> {
    static CELL: OnceLock<Mutex<HashMap<u64, ParsedData>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(HashMap::new()))
}

fn extract_gps_points(input: &crate::Input) -> Vec<GpsPoint> {
    let mut points = Vec::new();
    let samples = match &input.samples {
        Some(s) => s,
        None => return points,
    };

    let mut running_ts = 0.0f64;

    for info in samples {
        if info.tag_map.is_none() {
            continue;
        }
        let grouped_tag_map = info.tag_map.as_ref().unwrap();
        let (gps_fix, gps_precision) = util::gpmf_gps_fix_precision(grouped_tag_map);

        for (group, map) in grouped_tag_map {
            if group != &GroupId::GPS {
                continue;
            }

            let utc_time: Option<u64> = map
                .iter()
                .find(|(_, v)| v.description == "GPSU")
                .and_then(|(_, t)| {
                    if let TagValue::u64(tv) = &t.value {
                        Some(*tv.get())
                    } else {
                        None
                    }
                });

            if let Some(gps5) = map.get(&TagId::Data) {
                match &gps5.value {
                    TagValue::Vec_Vec_i32(gpsdata) => {
                        let data = gpsdata.get();
                        let duration_per_point = if data.len() > 1 {
                            info.duration_ms / 1000.0 / data.len() as f64
                        } else {
                            0.0
                        };
                        for row in data {
                            if row.len() >= 5 {
                                let utc_ms = utc_time.unwrap_or(0) as i64;
                                points.push(GpsPoint {
                                    latitude: row[0] as f64 / 10_000_000.0,
                                    longitude: row[1] as f64 / 10_000_000.0,
                                    altitude: row[2] as f64 / 1000.0,
                                    speed2d: row[3] as f64 / 1000.0,
                                    timestamp_s: running_ts,
                                    utc_time_ms: utc_ms,
                                    fix: gps_fix,
                                    precision: gps_precision,
                                });
                                running_ts += duration_per_point;
                            }
                        }
                    }
                    TagValue::Vec_GpsData(arr) => {
                        for g in arr.get() {
                            points.push(GpsPoint {
                                latitude: g.lat,
                                longitude: g.lon,
                                altitude: g.altitude,
                                speed2d: g.speed / 3.6,
                                timestamp_s: g.unix_timestamp,
                                utc_time_ms: (g.unix_timestamp * 1000.0) as i64,
                                fix: if g.is_acquired {
                                    gps_fix
                                } else {
                                    0
                                },
                                precision: gps_precision,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    points
}

pub fn parse_file(path: &str) -> Result<ParsedData, String> {
    let wrapper =
        filesystem::open_file(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let size = wrapper.size;
    let mut stream = BufReader::new(wrapper.file);

    let mut options = InputOptions::default();
    options.dont_look_for_sidecar_files = true;

    let input = crate::Input::from_stream_with_options(
        &mut stream,
        size,
        std::path::Path::new(path),
        |_| {},
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        options,
    )
    .map_err(|e| format!("Failed to parse: {}", e))?;

    let camera_type = input.camera_type();

    let gps_points = extract_gps_points(&input);
    let imu_data = util::normalized_imu(&input, None).unwrap_or_default();

    let lens_json = gpmf_lens::extract_lens(&input).and_then(|l| serde_json::to_string(&l).ok());

    let (cori, iori, orientation_combined) = input
        .gopro_orientation_streams_ns()
        .map(|(a, b, c)| {
            (
                a.iter().map(|(t, q)| (*t, q.clone())).collect(),
                b.iter().map(|(t, q)| (*t, q.clone())).collect(),
                c.iter().map(|(t, q)| (*t, q.clone())).collect(),
            )
        })
        .unwrap_or_else(|| (Vec::new(), Vec::new(), Vec::new()));

    let (video_info, duration_s) = {
        let mut stream2 = filesystem::open_file(path).ok();
        let mut md = VideoMetadata::default();
        if let Some(ref mut w) = stream2 {
            if let Ok(m) = util::get_video_metadata(&mut w.file, w.size) {
                md = m;
            }
        }
        let info = if md.fps > 0.0 {
            let frame_count = (md.duration_s * md.fps).round() as usize;
            Some((frame_count, md.fps))
        } else {
            None
        };
        (info, md.duration_s)
    };

    Ok(ParsedData {
        camera_type,
        duration_s,
        video_info,
        gps_points,
        imu_data,
        lens_json,
        cori,
        iori,
        orientation_combined,
    })
}

pub fn open(path: &str) -> Result<u64, String> {
    let data = parse_file(path)?;
    let handle = HANDLE_COUNTER.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut map) = parsed_data_map().lock() {
        map.insert(handle, data);
    }
    Ok(handle)
}

pub fn close(handle: u64) {
    if handle != 0 {
        if let Ok(mut map) = parsed_data_map().lock() {
            map.remove(&handle);
        }
    }
}

pub fn with_handle<T, F>(handle: u64, f: F) -> Option<T>
where
    F: FnOnce(&ParsedData) -> T,
{
    if handle == 0 {
        return None;
    }
    parsed_data_map()
        .lock()
        .ok()
        .and_then(|map| map.get(&handle).map(f))
}
