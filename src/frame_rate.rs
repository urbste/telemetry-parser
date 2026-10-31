// SPDX-License-Identifier: MIT OR Apache-2.0
// Portable video frame-rate conversion shared by mobile parser bindings.

use serde::Serialize;

/// A positive frame cadence represented exactly as frames per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RationalFrameRate {
    pub numerator: i32,
    pub denominator: i32,
}

impl RationalFrameRate {
    /// Convert a finite frame rate to a compact rational representation.
    ///
    /// Integer rates are preserved. Common NTSC rates are normalized to their
    /// exact 1000/1001 cadence. Other rates are rounded to hundredths.
    pub fn from_fps(fps: f64) -> Option<Self> {
        if !fps.is_finite() || fps <= 0.0 || fps > 240.0 {
            return None;
        }

        let nearest_integer = fps.round();
        if nearest_integer >= 1.0 && (fps - nearest_integer).abs() < 0.01 {
            return Some(Self {
                numerator: nearest_integer as i32,
                denominator: 1,
            });
        }

        let ntsc_base = fps * 1001.0 / 1000.0;
        let nearest_ntsc_base = ntsc_base.round();
        if (1.0..=240.0).contains(&nearest_ntsc_base)
            && (ntsc_base - nearest_ntsc_base).abs() < 0.01
        {
            return Some(Self {
                numerator: nearest_ntsc_base as i32 * 1000,
                denominator: 1001,
            });
        }

        let hundredths = (fps * 100.0).round();
        if hundredths < 1.0 || hundredths > i32::MAX as f64 {
            return None;
        }
        let hundredths = hundredths as i32;
        let divisor = gcd(hundredths, 100);
        Some(Self {
            numerator: hundredths / divisor,
            denominator: 100 / divisor,
        })
    }

    pub fn frame_count_for_duration(self, duration_s: f64) -> Option<i32> {
        if !duration_s.is_finite() || duration_s < 0.0 {
            return None;
        }
        let count = duration_s * self.numerator as f64 / self.denominator as f64;
        if !count.is_finite() || count > i32::MAX as f64 {
            return None;
        }
        Some(count.round() as i32)
    }
}

fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct VideoInfoJson {
    #[serde(rename = "frameCount")]
    pub frame_count: i32,
    #[serde(rename = "fpsNumerator")]
    pub fps_numerator: i32,
    #[serde(rename = "fpsDenominator")]
    pub fps_denominator: i32,
}

impl VideoInfoJson {
    pub fn from_duration(duration_s: f64, fps: f64) -> Option<Self> {
        let rate = RationalFrameRate::from_fps(fps)?;
        Some(Self {
            frame_count: rate.frame_count_for_duration(duration_s)?,
            fps_numerator: rate.numerator,
            fps_denominator: rate.denominator,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_integer_and_ntsc_rates_to_exact_rationals() {
        for (fps, numerator, denominator) in [
            (24.0, 24, 1),
            (25.0, 25, 1),
            (30.0, 30, 1),
            (50.0, 50, 1),
            (60.0, 60, 1),
            (24000.0 / 1001.0, 24000, 1001),
            (30000.0 / 1001.0, 30000, 1001),
            (60000.0 / 1001.0, 60000, 1001),
            (29.97, 30000, 1001),
            (59.94, 60000, 1001),
        ] {
            assert_eq!(
                RationalFrameRate::from_fps(fps),
                Some(RationalFrameRate {
                    numerator,
                    denominator
                }),
                "fps={fps}"
            );
        }
    }

    #[test]
    fn rejects_non_finite_non_positive_and_out_of_range_rates() {
        for fps in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0, 241.0] {
            assert_eq!(RationalFrameRate::from_fps(fps), None, "fps={fps}");
        }
    }

    #[test]
    fn json_and_duration_count_use_the_same_rational_rate() {
        let info = VideoInfoJson::from_duration(60.0, 29.97).unwrap();
        assert_eq!(info.frame_count, 1798);
        assert_eq!(info.fps_numerator, 30000);
        assert_eq!(info.fps_denominator, 1001);
        assert_eq!(
            serde_json::to_string(&info).unwrap(),
            r#"{"frameCount":1798,"fpsNumerator":30000,"fpsDenominator":1001}"#
        );
        assert_eq!(
            VideoInfoJson::from_duration(60.0, 59.94)
                .unwrap()
                .frame_count,
            3596
        );
    }

    #[test]
    fn invalid_durations_do_not_produce_video_info() {
        assert_eq!(VideoInfoJson::from_duration(f64::NAN, 30.0), None);
        assert_eq!(VideoInfoJson::from_duration(-1.0, 30.0), None);
    }
}
