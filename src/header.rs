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

/// The drawing unit a `$INSUNITS` code names -- the model's table.
pub use uncad_model::Units;

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
