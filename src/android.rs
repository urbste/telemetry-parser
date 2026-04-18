// SPDX-License-Identifier: MIT OR Apache-2.0
// Android JNI bindings for telemetry-parser

#[cfg(target_os = "android")]
mod implementation {
    use jni::objects::{JObject, JString, JValue};
    use jni::sys::{jfloat, jint, jlong, jobject, jobjectArray, JavaVM};
    use jni::strings::JNIString;
    use jni::{jni_sig, jni_str, Env, EnvUnowned};
    use std::collections::HashMap;
    use std::io::BufReader;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};

    use crate::filesystem;
    use crate::gpmf_lens;
    use crate::tags_impl::*;
    use crate::util::{self, IMUData, VideoMetadata};
    use crate::InputOptions;

    static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct ParsedData {
        camera_type: String,
        duration_s: f64,
        video_info: Option<(usize, f64)>, // (frame_count, fps)
        gps_points: Vec<GpsPoint>,
        imu_data: Vec<IMUData>,
        lens_json: Option<String>,
        cori: Vec<(i64, crate::tags_impl::Quaternion<f64>)>,
        iori: Vec<(i64, crate::tags_impl::Quaternion<f64>)>,
        orientation_combined: Vec<(i64, crate::tags_impl::Quaternion<f64>)>,
    }

    #[derive(Clone)]
    struct GpsPoint {
        latitude: f64,
        longitude: f64,
        altitude: f64,
        speed2d: f64,
        timestamp_s: f64,
        utc_time_ms: i64,
        fix: i32,
        precision: i32,
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

    fn parse_file(path: &str) -> Result<ParsedData, String> {
        let wrapper = filesystem::open_file(path)
            .map_err(|e| format!("Failed to open file: {}", e))?;
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
        let imu_data = util::normalized_imu(&input, None)
            .unwrap_or_default();

        let lens_json = gpmf_lens::extract_lens(&input).and_then(|l| {
            serde_json::to_string(&l).ok()
        });

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

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeOpen(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        path: JString<'_>,
    ) -> jlong {
        unowned_env.with_env(|env| -> Result<jlong, jni::errors::Error> {
            let path_str: String = path.try_to_string(env).unwrap_or_default();
            match parse_file(&path_str) {
                Ok(data) => {
                    let handle = HANDLE_COUNTER.fetch_add(1, Ordering::SeqCst);
                    if let Ok(mut map) = parsed_data_map().lock() {
                        map.insert(handle, data);
                    }
                    Ok(handle as jlong)
                }
                Err(e) => {
                    let _ = env.throw_new(jni_str!("java/io/IOException"), JNIString::new(&e));
                    #[cfg(debug_assertions)]
                    eprintln!("telemetry-parser: failed to throw: {}", e);
                    Ok(0)
                }
            }
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeClose(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        handle: jlong,
    ) {
        let _ = unowned_env
            .with_env(|_env| -> Result<(), jni::errors::Error> {
                if handle != 0 {
                    if let Ok(mut map) = parsed_data_map().lock() {
                        map.remove(&(handle as u64));
                    }
                }
                Ok(())
            })
            .resolve::<jni::errors::ThrowRuntimeExAndDefault>();
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeGetCameraType(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        handle: jlong,
    ) -> jobject {
        unowned_env
            .with_env(|env| -> Result<jobject, jni::errors::Error> {
                if handle == 0 {
                    return Ok(std::ptr::null_mut());
                }
                if let Ok(map) = parsed_data_map().lock() {
                    if let Some(data) = map.get(&(handle as u64)) {
                        if let Ok(s) = env.new_string(&data.camera_type) {
                            return Ok(s.into_raw());
                        }
                    }
                }
                Ok(std::ptr::null_mut())
            })
            .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeGetDuration(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        handle: jlong,
    ) -> jfloat {
        unowned_env
            .with_env(|_env| -> Result<jfloat, jni::errors::Error> {
                if handle == 0 {
                    return Ok(0.0);
                }
                if let Ok(map) = parsed_data_map().lock() {
                    if let Some(data) = map.get(&(handle as u64)) {
                        return Ok(data.duration_s as jfloat);
                    }
                }
                Ok(0.0)
            })
            .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeGetVideoInfo(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        handle: jlong,
    ) -> jobject {
        unowned_env
            .with_env(|env| -> Result<jobject, jni::errors::Error> {
                if handle == 0 {
                    return Ok(std::ptr::null_mut());
                }
                if let Ok(map) = parsed_data_map().lock() {
                    if let Some(data) = map.get(&(handle as u64)) {
                        if let Some((frame_count, fps)) = data.video_info {
                            let num = fps.round() as i32;
                            let den = if (fps - fps.floor()).abs() < 0.01 { 1 } else { 1001 };
                            if let Ok(obj) = env.new_object(
                                jni_str!("io/github/telemetryparser/model/VideoInfo"),
                                jni_sig!("(III)V"),
                                &[
                                    JValue::Int(frame_count as i32),
                                    JValue::Int(num),
                                    JValue::Int(den),
                                ],
                            ) {
                                return Ok(obj.into_raw());
                            }
                        }
                    }
                }
                Ok(std::ptr::null_mut())
            })
            .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeGetGpsData(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        handle: jlong,
    ) -> jobjectArray {
        unowned_env
            .with_env(|env| extract_gps_array(env, handle))
            .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeGetAccelerometerData(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        handle: jlong,
    ) -> jobjectArray {
        unowned_env
            .with_env(|env| extract_imu_array(env, handle, |d| d.accl))
            .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeGetGyroscopeData(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        handle: jlong,
    ) -> jobjectArray {
        unowned_env
            .with_env(|env| extract_imu_array(env, handle, |d| d.gyro))
            .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeGetGravityVectorData(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        handle: jlong,
    ) -> jobjectArray {
        unowned_env
            .with_env(|env| extract_imu_array(env, handle, |d| d.grav))
            .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeGetLensMetadataJson(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        handle: jlong,
    ) -> jobject {
        unowned_env
            .with_env(|env| -> Result<jobject, jni::errors::Error> {
                if handle == 0 {
                    return Ok(std::ptr::null_mut());
                }
                if let Ok(map) = parsed_data_map().lock() {
                    if let Some(data) = map.get(&(handle as u64)) {
                        if let Some(ref js) = data.lens_json {
                            let s = env.new_string(js)?;
                            return Ok(s.into_raw());
                        }
                    }
                }
                Ok(std::ptr::null_mut())
            })
            .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeGetCameraOrientationData(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        handle: jlong,
    ) -> jobjectArray {
        unowned_env
            .with_env(|env| extract_orientation_array(env, handle, |d| d.cori.as_slice()))
            .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeGetImageOrientationData(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        handle: jlong,
    ) -> jobjectArray {
        unowned_env
            .with_env(|env| extract_orientation_array(env, handle, |d| d.iori.as_slice()))
            .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeGetOrientationCombinedData(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        handle: jlong,
    ) -> jobjectArray {
        unowned_env
            .with_env(|env| {
                extract_orientation_array(env, handle, |d| d.orientation_combined.as_slice())
            })
            .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
    }

    fn extract_gps_array(env: &mut Env<'_>, handle: jlong) -> Result<jobjectArray, jni::errors::Error> {
        if handle == 0 {
            return Ok(std::ptr::null_mut());
        }
        if let Ok(map) = parsed_data_map().lock() {
            if let Some(data) = map.get(&(handle as u64)) {
                let class = env.find_class(jni_str!("io/github/telemetryparser/model/GpsPoint"))?;
                let arr = env.new_object_array(data.gps_points.len() as i32, &class, JObject::null())?;
                for (i, pt) in data.gps_points.iter().enumerate() {
                    let obj = env.new_object(
                        jni_str!("io/github/telemetryparser/model/GpsPoint"),
                        jni_sig!("(DDDDDIJII)V"),
                        &[
                            JValue::Double(pt.latitude),
                            JValue::Double(pt.longitude),
                            JValue::Double(pt.altitude),
                            JValue::Double(pt.speed2d),
                            JValue::Double(pt.timestamp_s),
                            JValue::Long(pt.utc_time_ms),
                            JValue::Int(pt.fix),
                            JValue::Int(pt.precision),
                        ],
                    )?;
                    arr.set_element(env, i, &obj)?;
                }
                return Ok(arr.into_raw());
            }
        }
        Ok(std::ptr::null_mut())
    }

    fn extract_imu_array(
        env: &mut Env<'_>,
        handle: jlong,
        get_vals: fn(&IMUData) -> Option<[f64; 3]>,
    ) -> Result<jobjectArray, jni::errors::Error> {
        if handle == 0 {
            return Ok(std::ptr::null_mut());
        }
        if let Ok(map) = parsed_data_map().lock() {
            if let Some(data) = map.get(&(handle as u64)) {
                let filtered: Vec<_> = data
                    .imu_data
                    .iter()
                    .filter_map(|d| {
                        get_vals(d).map(|v| {
                            let ts_ns = (d.timestamp_ms * 1_000_000.0).round() as i64;
                            (ts_ns, v)
                        })
                    })
                    .collect();
                let class = env.find_class(jni_str!("io/github/telemetryparser/model/SensorSample"))?;
                let arr = env.new_object_array(filtered.len() as i32, &class, JObject::null())?;
                for (i, (ts_ns, v)) in filtered.iter().enumerate() {
                    let obj = env.new_object(
                        jni_str!("io/github/telemetryparser/model/SensorSample"),
                        jni_sig!("(JFFF)V"),
                        &[
                            JValue::Long(*ts_ns),
                            JValue::Float(v[0] as f32),
                            JValue::Float(v[1] as f32),
                            JValue::Float(v[2] as f32),
                        ],
                    )?;
                    arr.set_element(env, i, &obj)?;
                }
                return Ok(arr.into_raw());
            }
        }
        Ok(std::ptr::null_mut())
    }

    fn extract_orientation_array<F>(
        env: &mut Env<'_>,
        handle: jlong,
        get_vec: F,
    ) -> Result<jobjectArray, jni::errors::Error>
    where
        F: Fn(&ParsedData) -> &[(i64, crate::tags_impl::Quaternion<f64>)],
    {
        if handle == 0 {
            return Ok(std::ptr::null_mut());
        }
        if let Ok(map) = parsed_data_map().lock() {
            if let Some(data) = map.get(&(handle as u64)) {
                let v = get_vec(data);
                let class = env.find_class(jni_str!("io/github/telemetryparser/model/OrientationSample"))?;
                let arr = env.new_object_array(v.len() as i32, &class, JObject::null())?;
                for (i, (ts_ns, q)) in v.iter().enumerate() {
                    let e = crate::util::gpmf_orientation_quaternion_export(q);
                    let obj = env.new_object(
                        jni_str!("io/github/telemetryparser/model/OrientationSample"),
                        jni_sig!("(JDDDD)V"),
                        &[
                            JValue::Long(*ts_ns),
                            JValue::Double(e[0]),
                            JValue::Double(e[1]),
                            JValue::Double(e[2]),
                            JValue::Double(e[3]),
                        ],
                    )?;
                    arr.set_element(env, i, &obj)?;
                }
                return Ok(arr.into_raw());
            }
        }
        Ok(std::ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn JNI_OnLoad(_vm: JavaVM, _: *mut std::ffi::c_void) -> jint {
        jni::sys::JNI_VERSION_1_6
    }
}

// Stub for non-Android targets so the library still compiles
#[cfg(not(target_os = "android"))]
mod implementation {
    // No-op; android module is only used when building for Android
}
