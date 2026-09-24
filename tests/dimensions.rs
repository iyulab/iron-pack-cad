//! Dimension values: the measurement a file stores and the one its
//! definition points give, both reported as facts with their difference,
//! and the label the drawing shows. Sources: the `dimlfac12_r2000.dxf`
//! fixture (see tests/fixtures/README.md) and dimensions built here.

mod common;

use common::*;
use iron_pack_cad::dimension::{
    cached_labels, display_text, measurement_from_points, stored_measurement, DimDefaults,
    DisplaySource, EffectiveStyle, ValueFrom,
};
use iron_pack_cad::ExportOptions;
use serde_json::{json, Value};
use uncad_model::model::{DimensionEntity, DimensionKind, DimensionPoints, Entity, TextOverride};
use uncad_model::{CadDatabase, Point2D, Ref};

fn dimensions(db: &CadDatabase) -> Vec<&DimensionEntity> {
    db.entities
        .iter()
        .filter_map(|e| match e {
            Entity::Dimension(d) => Some(d),
            _ => None,
        })
        .collect()
}

/// An ALIGNED dimension between (0, 0) and (3, 4) -- 5 units apart -- that
/// stores `stored`, with no cached block.
fn aligned(id: u64, stored: Option<f64>) -> Entity {
    Entity::Dimension(DimensionEntity {
        common: common(id),
        block_name: Ref::Absent,
        kind: Some(DimensionKind::Aligned),
        measurement: stored,
        text_override: TextOverride::Measured,
        definition_point: Some(p3(3.0, 4.0)),
        text_midpoint: Point2D { x: 1.5, y: 2.5 },
        points: DimensionPoints {
            extension1: Some(p3(0.0, 0.0)),
            extension2: Some(p3(3.0, 4.0)),
            radial: None,
            arc: None,
        },
        rotation: 0.0,
        text_rotation: 0.0,
        style_name: Ref::Absent,
        ordinate_axis: None,
    })
}

#[test]
fn the_fixture_dimension_reads_its_style_and_its_cached_label() {
    // One LINEAR dimension 10 units long under $DIMLFAC 12 (the STANDARD
    // style says 12 too), storing act_measurement 10.0: the cached label
    // AutoCAD would show is "120", and a label formatted here from the same
    // style says the same.
    let (db, header) = fixture("dimlfac12_r2000.dxf");
    let dims = dimensions(&db);
    assert_eq!(dims.len(), 1);
    let d = dims[0];
    assert_eq!(d.kind, Some(DimensionKind::Rotated));
    assert_eq!(measurement_from_points(d), Some(10.0));
    assert_eq!(stored_measurement(d.measurement, d.kind), Some(10.0));
    let style = EffectiveStyle::resolve(
        d.style_name
            .resolved()
            .and_then(|name| db.tables.dim_styles.get(name)),
        &DimDefaults::from_header(&header),
    );
    assert_eq!(style.dimlfac, 12.0);
    let labels = cached_labels(&db.tables);
    let shown = display_text(
        d,
        &style,
        measurement_from_points(d),
        stored_measurement(d.measurement, d.kind),
        labels.get(d.block_name.name()),
    );
    assert_eq!(
        (shown.text.as_str(), shown.source, shown.value_from),
        ("120", DisplaySource::CachedBlock, None)
    );
    let formatted = iron_pack_cad::dimension::format_measurement(10.0, false, &style);
    assert!(formatted.starts_with("120"), "{formatted}");

    // And the package says so.
    let tmp = TempDir::new("dimlfac12");
    export_fixture("dimlfac12_r2000.dxf", &tmp.0, &ExportOptions::default()).expect("exports");
    let record = &records(&tmp.0, "dimensions")[0];
    assert_eq!(record["measurement_stored"], 10.0);
    assert_eq!(record["measurement_from_points"], 10.0);
    assert_eq!(record["delta"], 0.0);
    assert_eq!(record["dimlfac"], 12.0);
    assert_eq!(record["display"], "120");
    assert_eq!(record["display_source"], "cached_block");
}

#[test]
fn a_package_reports_both_values_and_does_not_choose() {
    // Four dimensions over the same 5-unit pair: one storing 5 exactly,
    // one storing a value 3e-4 off (a real disagreement a file can carry),
    // one storing a value of something else entirely (500), and one
    // storing nothing -- the R13/R14 shape. Every record carries what the
    // file stores and what the points give, as they are, and neither is
    // called the value.
    let db = model_space(vec![
        aligned(0x10, Some(5.0)),
        aligned(0x11, Some(5.0003)),
        aligned(0x12, Some(500.0)),
        aligned(0x13, None),
        // A label, so the drawing has zoom levels and tiles.
        text(0x30, 0.0, -1.0, 0.2, "A"),
    ]);
    let tmp = TempDir::new("two_values");
    export_db(&db, &tmp.0, &ExportOptions::default()).expect("exports");
    let dims = records(&tmp.0, "dimensions");
    assert_eq!(dims.len(), 4);
    for (handle, stored, delta) in [
        ("10", json!(5.0), json!(0.0)),
        ("11", json!(5.0003), json!(0.0003)),
        ("12", json!(500.0), json!(495.0)),
        ("13", Value::Null, Value::Null),
    ] {
        let d = by_handle(&dims, handle);
        assert_eq!(d["measurement_stored"], stored, "{handle}");
        assert_eq!(d["measurement_from_points"], 5.0, "{handle}");
        assert_eq!(d["delta"], delta, "{handle}");
        for gone in ["measurement", "measurement_source", "confidence"] {
            assert!(d.get(gone).is_none(), "{handle} still carries {gone}");
        }
        // No cached block: the label is formatted, from the points.
        assert_eq!(d["display_source"], "formatted", "{handle}");
        assert_eq!(d["display_value_from"], "points", "{handle}");
    }
    let manifest = read_json(&tmp.0.join("manifest.json"));
    assert_eq!(
        manifest["capabilities"]["dimension_values"],
        json!({"stored": 3, "from_points": 4})
    );
    let legend = &manifest["legend"];
    assert!(legend["confidence"]["stored"].is_null());
    assert!(!legend["confidence"]["carried_by"]
        .as_array()
        .unwrap()
        .contains(&json!("dimension")));
    assert!(legend["dimension_values"]
        .as_str()
        .is_some_and(|s| s.contains("neither is chosen")));
    // A tile's dimension rows carry both values too.
    let tiles = read_json(&tmp.0.join("tiles.json"));
    let sidecar = tiles["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|t| t["sidecar"].as_str())
        .expect("a written tile");
    let sidecar = read_json(&tmp.0.join(sidecar));
    assert_eq!(
        sidecar["columns"]["dims"],
        json!([
            "id",
            "px_box",
            "display",
            "measurement_stored",
            "measurement_from_points"
        ])
    );
}

#[test]
fn a_label_is_formatted_from_the_stored_value_only_when_the_points_give_none() {
    // A dimension of no known kind has no value from its points: the label
    // this crate formats is the stored one, and says so.
    let mut unknown = aligned(0x20, Some(7.5));
    if let Entity::Dimension(d) = &mut unknown {
        d.kind = None;
    }
    let tmp = TempDir::new("stored_label");
    export_db(
        &model_space(vec![unknown]),
        &tmp.0,
        &ExportOptions::default(),
    )
    .expect("exports");
    let d = &records(&tmp.0, "dimensions")[0];
    assert_eq!(d["measurement_from_points"], Value::Null);
    assert_eq!(d["measurement_stored"], 7.5);
    assert_eq!(d["display_value_from"], "stored");
    assert_eq!(
        d["display_source"],
        serde_json::to_value(DisplaySource::Formatted).unwrap()
    );
    assert_eq!(
        serde_json::to_value(ValueFrom::Stored).unwrap(),
        d["display_value_from"]
    );
}
