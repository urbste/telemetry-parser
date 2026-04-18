// SPDX-License-Identifier: MIT OR Apache-2.0
// Extract GoPro/action camera metadata to JSON in gpmf_android-compatible format

use argh::FromArgs;
use serde::Serialize;
use std::io::BufReader;
use std::sync::{atomic::AtomicBool, Arc};

use telemetry_parser::filesystem;
use telemetry_parser::gpmf_lens::{self, LensJson};
use telemetry_parser::tags_impl::*;
use telemetry_parser::util::{self, VideoMetadata};
use telemetry_parser::InputOptions;

/// JSON output: `camera_type`, `video_info`, `lens` preserved; IMU/orientation as `[[x,y,z],…]` + parallel `*_timestamps_ns`.
#[derive(Serialize)]
struct Output {
    camera_type: String,
    video_info: Option<VideoInfoJson>,
    /// GoPro DVID / FOVL lens calibration (when present in GPMF).
    #[serde(skip_serializing_if = "Option::is_none")]
    lens: Option<LensJson>,
    /// Per point: `[latitude °, longitude °, altitude m, speed2d m/s, fifth]`. Fifth is GPS5 column 5 (scaled), or `track`° for `Vec_GpsData`.
    gps_llh: Vec<[f64; 5]>,
    /// Clip-relative time per point, nanoseconds (`f64` to preserve fractional timing).
    gps_timestamps_ns: Vec<f64>,
    gps_precision: Vec<f64>,
    gps_fix: Vec<f64>,
    accelerometer: Vec<[f64; 3]>,
    accelerometer_timestamps_ns: Vec<i64>,
    gyroscope: Vec<[f64; 3]>,
    gyroscope_timestamps_ns: Vec<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gravity: Option<Vec<[f64; 3]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gravity_timestamps_ns: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    camera_orientation: Option<Vec<[f64; 4]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    camera_orientation_timestamps_ns: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_orientation: Option<Vec<[f64; 4]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_orientation_timestamps_ns: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    orientation_combined: Option<Vec<[f64; 4]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    orientation_combined_timestamps_ns: Option<Vec<i64>>,
}

#[derive(Serialize)]
struct VideoInfoJson {
    frame_count: usize,
    fps_numerator: i32,
    fps_denominator: i32,
}

#[derive(FromArgs)]
/// Extract metadata from action camera video to JSON (gpmf_android format)
struct Opts {
    /// output JSON path (default: same folder as input, with _metadata.json suffix)
    #[argh(option, short = 'o')]
    output: Option<String>,
    /// input video file
    #[argh(positional)]
    file: String,
}

fn main() {
    let opts: Opts = argh::from_env();

    let wrapper = filesystem::open_file(&opts.file).expect("Failed to open file");
    let size = wrapper.size;
    let mut stream = BufReader::new(wrapper.file);

    let mut options = InputOptions::default();
    options.dont_look_for_sidecar_files = true;

    let input = telemetry_parser::Input::from_stream_with_options(
        &mut stream,
        size,
        std::path::Path::new(&opts.file),
        |_| {},
        Arc::new(AtomicBool::new(false)),
        options,
    )
    .expect("Failed to parse file");

    let camera_type = input.camera_type();

    let (gps_llh, gps_timestamps_ns, gps_precision, gps_fix) = extract_gps_series(&input);
    let imu_data = util::normalized_imu(&input, None).unwrap_or_default();

    let (accelerometer, accelerometer_timestamps_ns) =
        triple_stream_from_imu(&imu_data, |d| d.accl);
    let (gyroscope, gyroscope_timestamps_ns) =
        triple_stream_from_imu(&imu_data, |d| d.gyro);
    let (grav_v, grav_t) = triple_stream_from_imu(&imu_data, |d| d.grav);
    let (gravity, gravity_timestamps_ns) = if grav_v.is_empty() {
        (None, None)
    } else {
        (Some(grav_v), Some(grav_t))
    };

    let (camera_orientation, camera_orientation_timestamps_ns, image_orientation, image_orientation_timestamps_ns, orientation_combined, orientation_combined_timestamps_ns) =
        match input.gopro_orientation_streams_ns() {
            Some((c, i, o)) => {
                let (cv, ct) = quaternion_stream_from_pairs(c);
                let (iv, it) = quaternion_stream_from_pairs(i);
                let (ov, ot) = quaternion_stream_from_pairs(o);
                (
                    Some(cv),
                    Some(ct),
                    Some(iv),
                    Some(it),
                    Some(ov),
                    Some(ot),
                )
            }
            None => (None, None, None, None, None, None),
        };

    let (video_info, _) = {
        let mut w = filesystem::open_file(&opts.file).ok();
        let mut md = VideoMetadata::default();
        if let Some(ref mut wrapper) = w {
            if let Ok(m) = util::get_video_metadata(&mut wrapper.file, wrapper.size) {
                md = m;
            }
        }
        let info = if md.fps > 0.0 {
            let frame_count = (md.duration_s * md.fps).round() as usize;
            let num = md.fps.round() as i32;
            let den = if (md.fps - md.fps.floor()).abs() < 0.01 {
                1
            } else {
                1001
            };
            Some(VideoInfoJson {
                frame_count,
                fps_numerator: num,
                fps_denominator: den,
            })
        } else {
            None
        };
        (info, md.duration_s)
    };

    let lens = gpmf_lens::extract_lens(&input);

    let output = Output {
        camera_type: camera_type.clone(),
        video_info,
        lens,
        gps_llh,
        gps_timestamps_ns,
        gps_precision,
        gps_fix,
        accelerometer,
        accelerometer_timestamps_ns,
        gyroscope,
        gyroscope_timestamps_ns,
        gravity,
        gravity_timestamps_ns,
        camera_orientation,
        camera_orientation_timestamps_ns,
        image_orientation,
        image_orientation_timestamps_ns,
        orientation_combined,
        orientation_combined_timestamps_ns,
    };

    let out_path = opts.output.unwrap_or_else(|| {
        let stem = std::path::Path::new(&opts.file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("metadata")
            .to_string();
        let dir = std::path::Path::new(&opts.file).parent().unwrap_or(std::path::Path::new("."));
        dir.join(format!("{}_metadata.json", stem)).to_string_lossy().to_string()
    });

    let json = serde_json::to_string_pretty(&output).expect("Serialize");
    std::fs::write(&out_path, json).expect("Write output");
    let total = output.gps_llh.len()
        + output.accelerometer.len()
        + output.gyroscope.len()
        + output.gravity.as_ref().map(|g| g.len()).unwrap_or(0);
    eprintln!(
        "Wrote {} samples ({} gps, {} accel, {} gyro){} to {}",
        total,
        output.gps_llh.len(),
        output.accelerometer.len(),
        output.gyroscope.len(),
        if output.lens.is_some() {
            " + lens"
        } else {
            ""
        },
        out_path
    );
}

fn triple_stream_from_imu<F>(imu: &[util::IMUData], get: F) -> (Vec<[f64; 3]>, Vec<i64>)
where
    F: Fn(&util::IMUData) -> Option<[f64; 3]>,
{
    let mut vals = Vec::new();
    let mut timestamps_ns = Vec::new();
    for d in imu {
        if let Some(v) = get(d) {
            vals.push(v);
            timestamps_ns.push((d.timestamp_ms * 1_000_000.0).round() as i64);
        }
    }
    (vals, timestamps_ns)
}

fn quaternion_stream_from_pairs(
    v: &[(i64, telemetry_parser::tags_impl::Quaternion<f64>)],
) -> (Vec<[f64; 4]>, Vec<i64>) {
    (
        v.iter()
            .map(|(_, q)| util::gpmf_orientation_quaternion_export(q))
            .collect(),
        v.iter().map(|(t, _)| *t).collect(),
    )
}

/// Returns `(gps_llh, timestamps_ns, precision per point, fix per point)`.
fn extract_gps_series(
    input: &telemetry_parser::Input,
) -> (
    Vec<[f64; 5]>,
    Vec<f64>,
    Vec<f64>,
    Vec<f64>,
) {
    let mut gps_llh = Vec::new();
    let mut gps_timestamps_ns = Vec::new();
    let mut gps_precision = Vec::new();
    let mut gps_fix = Vec::new();

    let samples = match &input.samples {
        Some(s) => s,
        None => {
            return (gps_llh, gps_timestamps_ns, gps_precision, gps_fix);
        }
    };

    let mut running_ts_s = 0.0f64;

    for info in samples {
        if info.tag_map.is_none() {
            continue;
        }
        let grouped_tag_map = info.tag_map.as_ref().unwrap();
        let (fix_i, prec_i) = util::gpmf_gps_fix_precision(grouped_tag_map);
        let fix_f = fix_i as f64;
        let prec_f = prec_i as f64;

        for (group, map) in grouped_tag_map {
            if group != &GroupId::GPS {
                continue;
            }

            if let Some(gps5) = map.get(&TagId::Data) {
                match &gps5.value {
                    TagValue::Vec_Vec_i32(gpsdata) => {
                        let data = gpsdata.get();
                        let n = data.len().max(1);
                        let duration_per_point = info.duration_ms / 1000.0 / n as f64;
                        for row in data.iter() {
                            if row.len() >= 5 {
                                let lat = row[0] as f64 / 10_000_000.0;
                                let lon = row[1] as f64 / 10_000_000.0;
                                let alt = row[2] as f64 / 1000.0;
                                let speed2d = row[3] as f64 / 1000.0;
                                // GPS5 5th: typically 3D speed scale 1000 (m/s), same as 2D.
                                let fifth = row[4] as f64 / 1000.0;
                                gps_llh.push([lat, lon, alt, speed2d, fifth]);
                                gps_timestamps_ns.push(running_ts_s * 1_000_000_000.0);
                                gps_precision.push(prec_f);
                                gps_fix.push(fix_f);
                                running_ts_s += duration_per_point;
                            }
                        }
                    }
                    TagValue::Vec_GpsData(arr) => {
                        let data = arr.get();
                        let n = data.len().max(1);
                        let dt_s = info.duration_ms / 1000.0 / n as f64;
                        for g in data {
                            let speed2d = g.speed / 3.6;
                            let fifth = g.track;
                            gps_llh.push([g.lat, g.lon, g.altitude, speed2d, fifth]);
                            gps_timestamps_ns.push(running_ts_s * 1_000_000_000.0);
                            gps_precision.push(prec_f);
                            gps_fix.push(if g.is_acquired { fix_f } else { 0.0 });
                            running_ts_s += dt_s;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    (
        gps_llh,
        gps_timestamps_ns,
        gps_precision,
        gps_fix,
    )
}
