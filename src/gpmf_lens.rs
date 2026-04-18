// SPDX-License-Identifier: MIT OR Apache-2.0
// GoPro GPMF lens calibration (DVID / FOVL) extraction — shared by CLI and Android.

use crate::tags_impl::*;

/// Subset of DVID FOVL / lens FourCCs (ZFOV, VFOV, PYCF, POLY, …).
#[derive(serde::Serialize, Default, Clone)]
pub struct LensJson {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub vfov_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zfov_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absc: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zmpl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aruw: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arwa: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pycf_terms: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub poly_coeffs: Vec<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mxcf_terms: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mapx_coeffs: Vec<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mycf_terms: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mapy_coeffs: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dvid_primary: Option<String>,
}

fn merge_opt<T: Clone>(dst: &mut Option<T>, val: Option<T>) {
    if dst.is_none() {
        *dst = val;
    }
}

fn merge_vec<T: Clone>(dst: &mut Vec<T>, val: Vec<T>) {
    if dst.is_empty() && !val.is_empty() {
        *dst = val;
    }
}

fn scalar_f64(s: &Scalar) -> Option<f64> {
    match s {
        Scalar::f32(v) => Some(*v as f64),
        Scalar::f64(v) => Some(*v),
        Scalar::i8(v) => Some(*v as f64),
        Scalar::u8(v) => Some(*v as f64),
        Scalar::i16(v) => Some(*v as f64),
        Scalar::u16(v) => Some(*v as f64),
        Scalar::i32(v) => Some(*v as f64),
        Scalar::u32(v) => Some(*v as f64),
        Scalar::i64(v) => Some(*v as f64),
        Scalar::u64(v) => Some(*v as f64),
        _ => None,
    }
}

fn tag_f64(v: &TagValue) -> Option<f64> {
    match v {
        TagValue::f32(t) => Some(*t.get() as f64),
        TagValue::f64(t) => Some(*t.get()),
        TagValue::i8(t) => Some(*t.get() as f64),
        TagValue::u8(t) => Some(*t.get() as f64),
        TagValue::i16(t) => Some(*t.get() as f64),
        TagValue::u16(t) => Some(*t.get() as f64),
        TagValue::i32(t) => Some(*t.get() as f64),
        TagValue::u32(t) => Some(*t.get() as f64),
        TagValue::i64(t) => Some(*t.get() as f64),
        TagValue::u64(t) => Some(*t.get() as f64),
        _ => None,
    }
}

fn tag_string(v: &TagValue) -> Option<String> {
    match v {
        TagValue::String(t) => {
            let s = t.get().trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        _ => None,
    }
}

fn tag_vec_f64(v: &TagValue) -> Option<Vec<f64>> {
    match v {
        TagValue::f32(t) => Some(vec![*t.get() as f64]),
        TagValue::f64(t) => Some(vec![*t.get()]),
        TagValue::Vec_f32(t) => Some(t.get().iter().map(|x| *x as f64).collect()),
        TagValue::Vec_f64(t) => Some(t.get().clone()),
        TagValue::Vec_Vec_f32(t) => t
            .get()
            .iter()
            .find(|row| !row.is_empty())
            .map(|row| row.iter().map(|x| *x as f64).collect()),
        TagValue::Vec_Vec_f64(t) => t
            .get()
            .iter()
            .find(|row| !row.is_empty())
            .cloned(),
        TagValue::Vec_Scalar(t) => {
            let mut o = Vec::new();
            for s in t.get() {
                o.push(scalar_f64(s)?);
            }
            Some(o)
        }
        TagValue::Vec_Vec_Scalar(t) => {
            let row = t.get().iter().find(|r| !r.is_empty())?;
            let mut o = Vec::new();
            for s in row {
                o.push(scalar_f64(s)?);
            }
            Some(o)
        }
        _ => None,
    }
}

fn tag_vec_string(v: &TagValue) -> Option<Vec<String>> {
    match v {
        TagValue::Vec_String(t) => {
            let v = t
                .get()
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        }
        TagValue::String(t) => {
            let s = t.get().trim();
            if s.contains(',') {
                let v = s
                    .split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect::<Vec<_>>();
                if v.is_empty() {
                    None
                } else {
                    Some(v)
                }
            } else if s.is_empty() {
                None
            } else {
                Some(vec![s.to_string()])
            }
        }
        _ => None,
    }
}

pub fn extract_lens(input: &crate::Input) -> Option<LensJson> {
    let samples = input.samples.as_ref()?;
    let mut lens = LensJson::default();

    for info in samples {
        let Some(grouped) = &info.tag_map else {
            continue;
        };
        for (_group, map) in grouped {
            for (_id, desc) in map.iter() {
                match desc.description.as_str() {
                    "VFOV" => {
                        if lens.vfov_mode.is_empty() {
                            if let Some(s) = tag_string(&desc.value) {
                                lens.vfov_mode = s;
                            }
                        }
                    }
                    "ZFOV" => merge_opt(&mut lens.zfov_deg, tag_f64(&desc.value)),
                    "ABSC" => merge_opt(&mut lens.absc, tag_f64(&desc.value)),
                    "ZMPL" => merge_opt(&mut lens.zmpl, tag_f64(&desc.value)),
                    "ARUW" => merge_opt(&mut lens.aruw, tag_f64(&desc.value)),
                    "ARWA" => merge_opt(&mut lens.arwa, tag_f64(&desc.value)),
                    "PYCF" => {
                        if let Some(v) = tag_vec_string(&desc.value) {
                            merge_vec(&mut lens.pycf_terms, v);
                        }
                    }
                    "POLY" => {
                        if let Some(v) = tag_vec_f64(&desc.value) {
                            merge_vec(&mut lens.poly_coeffs, v);
                        }
                    }
                    "MXCF" => {
                        if let Some(v) = tag_vec_string(&desc.value) {
                            merge_vec(&mut lens.mxcf_terms, v);
                        }
                    }
                    "MAPX" => {
                        if let Some(v) = tag_vec_f64(&desc.value) {
                            merge_vec(&mut lens.mapx_coeffs, v);
                        }
                    }
                    "MYCF" => {
                        if let Some(v) = tag_vec_string(&desc.value) {
                            merge_vec(&mut lens.mycf_terms, v);
                        }
                    }
                    "MAPY" => {
                        if let Some(v) = tag_vec_f64(&desc.value) {
                            merge_vec(&mut lens.mapy_coeffs, v);
                        }
                    }
                    "DVID" => {
                        if lens.dvid_primary.is_none() {
                            if let Some(s) = tag_string(&desc.value) {
                                if s != "1" {
                                    lens.dvid_primary = Some(s);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let has_any = !lens.vfov_mode.is_empty()
        || lens.zfov_deg.is_some()
        || lens.absc.is_some()
        || lens.zmpl.is_some()
        || lens.aruw.is_some()
        || lens.arwa.is_some()
        || !lens.pycf_terms.is_empty()
        || !lens.poly_coeffs.is_empty()
        || !lens.mxcf_terms.is_empty()
        || !lens.mapx_coeffs.is_empty()
        || !lens.mycf_terms.is_empty()
        || !lens.mapy_coeffs.is_empty()
        || lens.dvid_primary.is_some();

    if has_any {
        Some(lens)
    } else {
        None
    }
}
