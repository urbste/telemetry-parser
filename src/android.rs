// SPDX-License-Identifier: MIT OR Apache-2.0
// Android JNI bindings for telemetry-parser

#[cfg(target_os = "android")]
mod implementation {
    use jni::objects::{JObject, JString, JValue};
    use jni::sys::{jfloat, jint, jlong, jobject, jobjectArray, JavaVM};
    use jni::strings::JNIString;
    use jni::{jni_sig, jni_str, Env, EnvUnowned};

    use crate::mobile::{self, GpsPoint, ParsedData};
    use crate::util::IMUData;

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_telemetryparser_TelemetryParser_nativeOpen(
        mut unowned_env: EnvUnowned<'_>,
        _: JObject,
        path: JString<'_>,
    ) -> jlong {
        unowned_env.with_env(|env| -> Result<jlong, jni::errors::Error> {
            let path_str: String = path.try_to_string(env).unwrap_or_default();
            match mobile::open(&path_str) {
                Ok(handle) => Ok(handle as jlong),
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
                mobile::close(handle as u64);
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
                mobile::with_handle(handle as u64, |data| {
                    env.new_string(&data.camera_type)
                        .map(|s| s.into_raw())
                        .unwrap_or(std::ptr::null_mut())
                })
                .unwrap_or(std::ptr::null_mut())
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
                Ok(mobile::with_handle(handle as u64, |data| data.duration_s as jfloat)
                    .unwrap_or(0.0))
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
                mobile::with_handle(handle as u64, |data| {
                    if let Some(info) = data.video_info_json() {
                        env.new_object(
                            jni_str!("io/github/telemetryparser/model/VideoInfo"),
                            jni_sig!("(III)V"),
                            &[
                                JValue::Int(info.frame_count),
                                JValue::Int(info.fps_numerator),
                                JValue::Int(info.fps_denominator),
                            ],
                        )
                        .map(|obj| obj.into_raw())
                        .unwrap_or(std::ptr::null_mut())
                    } else {
                        std::ptr::null_mut()
                    }
                })
                .unwrap_or(std::ptr::null_mut())
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
                mobile::with_handle(handle as u64, |data| {
                    if let Some(ref js) = data.lens_json {
                        env.new_string(js)
                            .map(|s| s.into_raw())
                            .unwrap_or(std::ptr::null_mut())
                    } else {
                        std::ptr::null_mut()
                    }
                })
                .unwrap_or(std::ptr::null_mut())
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
        let Some(result) = mobile::with_handle(handle as u64, |data| -> Result<jobjectArray, jni::errors::Error> {
            let class = env.find_class(jni_str!("io/github/telemetryparser/model/GpsPoint"))?;
            let arr = env.new_object_array(data.gps_points.len() as i32, &class, JObject::null())?;
            for (i, pt) in data.gps_points.iter().enumerate() {
                let obj = gps_point_to_jobject(env, pt)?;
                arr.set_element(env, i, &obj)?;
            }
            Ok(arr.into_raw())
        }) else {
            return Ok(std::ptr::null_mut());
        };
        result
    }

    fn gps_point_to_jobject(env: &mut Env<'_>, pt: &GpsPoint) -> Result<JObject<'_>, jni::errors::Error> {
        env.new_object(
            jni_str!("io/github/telemetryparser/model/GpsPoint"),
            jni_sig!("(DDDDDJII)V"),
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
        )
    }

    fn extract_imu_array(
        env: &mut Env<'_>,
        handle: jlong,
        get_vals: fn(&IMUData) -> Option<[f64; 3]>,
    ) -> Result<jobjectArray, jni::errors::Error> {
        let Some(result) = mobile::with_handle(handle as u64, |data| -> Result<jobjectArray, jni::errors::Error> {
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
            Ok(arr.into_raw())
        }) else {
            return Ok(std::ptr::null_mut());
        };
        result
    }

    fn extract_orientation_array<F>(
        env: &mut Env<'_>,
        handle: jlong,
        get_vec: F,
    ) -> Result<jobjectArray, jni::errors::Error>
    where
        F: Fn(&ParsedData) -> &[(i64, crate::tags_impl::Quaternion<f64>)],
    {
        let Some(result) = mobile::with_handle(handle as u64, |data| -> Result<jobjectArray, jni::errors::Error> {
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
            Ok(arr.into_raw())
        }) else {
            return Ok(std::ptr::null_mut());
        };
        result
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn JNI_OnLoad(_vm: JavaVM, _: *mut std::ffi::c_void) -> jint {
        jni::sys::JNI_VERSION_1_6
    }
}

#[cfg(not(target_os = "android"))]
mod implementation {}
