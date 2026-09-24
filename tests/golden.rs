//! Golden case G5 -- a drawing that is mostly dimensions, written by the
//! golden writer of the entity model -- read and packaged, and its dimension
//! records checked against the case's expected model: the measurement each
//! dimension states, the one its definition points give, their difference,
//! and the label the drawing shows. The copies in `tests/golden` are
//! byte-for-byte the writer's output (see its README).

mod common;

use common::*;
use iron_pack_cad::ExportOptions;
use serde_json::Value;
use uncad_model::model::{DimensionEntity, Entity, TextOverride};
use uncad_model::{CadDatabase, Ref};

fn expected() -> CadDatabase {
    serde_json::from_str(include_str!("golden/g5.expected.json"))
        .expect("the expected model deserializes")
}

fn handle(d: &DimensionEntity) -> String {
    match &d.common.source_handle {
        Ref::Resolved(h) => h.clone(),
        other => panic!("a golden dimension states its handle: {other:?}"),
    }
}

/// The text the dimension's drawn block holds -- what the drawing shows --
/// or `None` when the block draws no text.
fn block_text(db: &CadDatabase, d: &DimensionEntity) -> Option<String> {
    let Ref::Resolved(name) = &d.block_name else {
        return None;
    };
    db.tables.block_records[name]
        .entities
        .iter()
        .find_map(|e| match e {
            Entity::Text(t) if !t.text.is_empty() => Some(t.text.clone()),
            _ => None,
        })
}

#[test]
fn every_g5_dimension_carries_what_the_case_states() {
    let tmp = TempDir::new("golden_g5");
    export_golden("g5.dxf", &tmp.0, &ExportOptions::default()).expect("exports");
    let records = records(&tmp.0, "dimensions");
    let expected = expected();
    let dims: Vec<&DimensionEntity> = expected
        .entities
        .iter()
        .filter_map(|e| match e {
            Entity::Dimension(d) => Some(d),
            _ => None,
        })
        .collect();
    assert_eq!(records.len(), dims.len(), "one record per dimension");

    let mut seen = [false; 4];
    for d in dims {
        let h = handle(d);
        let r = by_handle(&records, &h);
        // The measurement the file states, exactly, or none.
        assert_eq!(
            r["measurement_stored"].as_f64(),
            d.measurement,
            "{h} stored"
        );
        // The value the definition points give, and the difference between
        // the two whenever both exist.
        let points = r["measurement_from_points"].as_f64().expect("points value");
        match r["measurement_stored"].as_f64() {
            Some(stored) => {
                let delta = r["delta"].as_f64().expect("a delta beside two values");
                assert!(
                    (delta - (stored - points)).abs() < 1e-6,
                    "{h} delta {delta}"
                );
            }
            None => assert!(r["delta"].is_null(), "{h} has one value, no delta"),
        }
        let display = r["display"].as_str().expect("a display string");
        let source = r["display_source"].as_str().expect("a display source");
        match (&d.text_override, block_text(&expected, d)) {
            (TextOverride::Suppressed, _) => {
                seen[0] = true;
                assert_eq!((display, source), ("", "suppressed"), "{h}");
            }
            (TextOverride::Literal(text), _) => {
                seen[1] = true;
                assert_eq!(source, "user_text", "{h}");
                // The file's control codes are decoded for reading: `%%C`
                // is the diameter sign, never shown as written.
                assert!(!display.contains("%%"), "{h} {display}");
                assert!(
                    display.ends_with(text.trim_start_matches("%%C")),
                    "{h} {display}"
                );
            }
            (TextOverride::Measured, Some(drawn)) => {
                seen[2] = true;
                assert_eq!((display, source), (drawn.as_str(), "cached_block"), "{h}");
            }
            (TextOverride::Measured, None) => {
                seen[3] = true;
                // Formatted from the style the dimension names: the value
                // at its decimal places, placed in the style's DIMPOST.
                let Ref::Resolved(style) = &d.style_name else {
                    panic!("{h}: a formatted label needs its style");
                };
                let style = &expected.tables.dim_styles[style];
                let places = style.decimal_places.expect("the case states DIMDEC") as usize;
                let value = format!("{points:.places$}");
                let want = style
                    .post
                    .as_deref()
                    .expect("the case states DIMPOST")
                    .replace("<>", &value);
                assert_eq!((display, source), (want.as_str(), "formatted"), "{h}");
            }
        }
    }
    assert_eq!(
        seen, [true; 4],
        "hidden, literal, drawn and formatted all met"
    );

    // The one dimension whose stated value disagrees with its own geometry
    // keeps both, and says by how much.
    let arc = records
        .iter()
        .find(|r| r["kind"] == "ARC_LENGTH")
        .expect("G5 has an arc-length dimension");
    assert!(
        arc["delta"].as_f64().is_some_and(|d| d.abs() > 1.0),
        "{arc}"
    );
    // Its style puts the arc symbol above the text; that is reported as the
    // style's fact, and the label itself does not carry the symbol.
    assert_eq!(arc["arc_symbol"], "ABOVE_TEXT", "{arc}");
    assert!(!arc["display"].as_str().unwrap_or("").contains('\u{2312}'));
    // Only an arc-length dimension carries the field.
    assert!(records
        .iter()
        .filter(|r| r["kind"] != "ARC_LENGTH")
        .all(|r| r.get("arc_symbol").is_none()));
}

#[test]
fn a_dimension_naming_an_undeclared_style_has_no_stored_value_to_trust() {
    let tmp = TempDir::new("golden_g5_style");
    export_golden("g5.dxf", &tmp.0, &ExportOptions::default()).expect("exports");
    let records = records(&tmp.0, "dimensions");
    let undeclared: Vec<&Value> = records
        .iter()
        .filter(|r| r["dimstyle"].as_str().is_none_or(str::is_empty))
        .collect();
    assert_eq!(undeclared.len(), 1, "one dimension names NOT-DECLARED");
    assert!(undeclared[0]["measurement_stored"].is_null());
    assert!(undeclared[0]["measurement_from_points"].is_number());
}
