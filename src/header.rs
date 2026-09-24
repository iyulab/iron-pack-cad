//! The drawing's header: the file-level facts and header variables that give
//! the model's numbers their meaning -- the unit a coordinate is in, the
//! extents the file claims, the dimension variables a DIMENSION falls back
//! on.
//!
//! [`uncad_model::CadDatabase`] carries no header variables, so whoever
//! reads the file hands them in beside it. This crate reads no files: a
//! [`Header`] is plain data, built by the caller or deserialized from JSON
//! whose field names are the DXF `$VARIABLE` names in lower case. The
//! variables the package reads are typed fields; every other key the JSON
//! holds is kept in [`Header::other`] and written back unchanged, so
//! `drawing.json` carries the header exactly as it was given.
//!
//! # Unknown is `None`
//!
//! A variable is `Some` only when the file states it. A default the format
//! documents is applied where the package uses the variable, never here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uncad_model::Point3D;

/// File-level facts and the header variables the package reads. Every field
/// is optional and defaults to `None` when deserialized; see the module doc.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Header {
    /// What the bytes were: `"dwg"` or `"dxf"`.
    pub format: Option<String>,
    /// The version code as the file states it (`"AC1015"`).
    pub acadver: Option<String>,
    /// A release name for that version (`"r2000"`).
    pub version: Option<String>,
    /// The name of the code page the drawing's 8-bit strings were decoded
    /// with (`"ANSI_1252"`).
    pub codepage_name: Option<String>,
    /// `$INSUNITS`: the drawing unit, as its code; see [`Header::units`].
    pub insunits: Option<u16>,
    /// `$LUPREC`: decimal places of linear units.
    pub luprec: Option<u16>,
    /// `$EXTMIN`/`$EXTMAX`: the model-space extents the file stores. May be
    /// stale, or AutoCAD's `+/-1e20` sentinel for "never set".
    pub extmin: Option<Point3D>,
    pub extmax: Option<Point3D>,
    /// `$DIMLFAC`: linear measurement factor of the current dimension style.
    pub dimlfac: Option<f64>,
    /// `$DIMDEC`: decimal places of a linear dimension.
    pub dimdec: Option<u16>,
    /// `$DIMLUNIT`: linear unit format of a dimension.
    pub dimlunit: Option<u16>,
    /// `$DIMPOST`: prefix/suffix of a dimension's text; `Some("")` when
    /// stated empty.
    pub dimpost: Option<String>,
    /// `$DIMRND`: rounding of a linear dimension.
    pub dimrnd: Option<f64>,
    /// `$DIMZIN`: zero suppression of a linear dimension.
    pub dimzin: Option<u16>,
    /// `$DIMADEC`: decimal places of an angular dimension.
    pub dimadec: Option<u16>,
    /// Every other key of the header this was deserialized from, written
    /// back unchanged.
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

impl Header {
    /// `$INSUNITS` as a unit name and a millimetre factor; `None` when the
    /// header does not state `$INSUNITS`.
    pub fn units(&self) -> Option<Units> {
        self.insunits.map(Units::from_insunits)
    }
}

/// The drawing unit a `$INSUNITS` code names, with its conversion to
/// millimetres when the code is a length.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Units {
    /// Short name: `"mm"`, `"in"`, `"ft"`, `"m"`, ... -- or `"du"` ("drawing
    /// units") for code 0 (unitless) and for a code the DXF reference does
    /// not define.
    pub name: String,
    /// Millimetres per drawing unit; `None` when unitless or unknown.
    pub to_mm: Option<f64>,
}

impl Units {
    /// The DXF reference's `$INSUNITS` table (codes 0..=24).
    pub fn from_insunits(code: u16) -> Units {
        let (name, to_mm): (&str, Option<f64>) = match code {
            1 => ("in", Some(25.4)),
            2 => ("ft", Some(304.8)),
            3 => ("mi", Some(1_609_344.0)),
            4 => ("mm", Some(1.0)),
            5 => ("cm", Some(10.0)),
            6 => ("m", Some(1000.0)),
            7 => ("km", Some(1_000_000.0)),
            8 => ("uin", Some(2.54e-5)),
            9 => ("mil", Some(0.0254)),
            10 => ("yd", Some(914.4)),
            11 => ("angstrom", Some(1e-7)),
            12 => ("nm", Some(1e-6)),
            13 => ("um", Some(1e-3)),
            14 => ("dm", Some(100.0)),
            15 => ("dam", Some(10_000.0)),
            16 => ("hm", Some(100_000.0)),
            17 => ("Gm", Some(1e12)),
            18 => ("au", Some(1.495_978_707e14)),
            19 => ("ly", Some(9.460_730_472_580_8e18)),
            20 => ("pc", Some(3.085_677_581_491_367e19)),
            // US survey units: 1200/3937 m to the foot.
            21 => ("us-ft", Some(304.800_609_601_219_2)),
            22 => ("us-in", Some(25.400_050_800_101_6)),
            23 => ("us-yd", Some(914.401_828_803_657_7)),
            24 => ("us-mi", Some(1_609_347.218_694_437_2)),
            _ => ("du", None),
        };
        Units {
            name: name.to_string(),
            to_mm,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_header_round_trips_the_keys_it_does_not_read() {
        let given = json!({
            "format": "dxf",
            "acadver": "AC1015",
            "insunits": 4,
            "dimscale": 2.5,
            "clayer": { "Resolved": "0" },
            "extmin": { "x": 0.0, "y": 0.0, "z": 0.0 },
            "extmax": null,
        });
        let header: Header = serde_json::from_value(given.clone()).unwrap();
        assert_eq!(header.insunits, Some(4));
        assert_eq!(header.units().unwrap().name, "mm");
        assert_eq!(header.other["dimscale"], json!(2.5));
        // Written back: every given key with its value, and the typed
        // fields the input left out as null.
        let back = serde_json::to_value(&header).unwrap();
        for (key, value) in given.as_object().unwrap() {
            assert_eq!(&back[key], value, "{key}");
        }
        assert_eq!(back["dimdec"], Value::Null);
    }

    #[test]
    fn an_empty_header_states_nothing() {
        let header: Header = serde_json::from_value(json!({})).unwrap();
        assert_eq!(header, Header::default());
        assert!(header.units().is_none());
    }
}
