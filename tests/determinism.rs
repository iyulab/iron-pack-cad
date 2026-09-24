//! Same input, same bytes: the whole package -- every file, `report.json`
//! included -- is identical on a second run, and the options change what
//! they say they change.

mod common;

use common::*;
use iron_pack_cad::{ExportOptions, Profile};

fn export_sample(dir: &std::path::Path, options: &ExportOptions) -> iron_pack_cad::ExportReport {
    iron_pack_cad::export_package(&sample_drawing(), Some(&sample_header()), dir, options)
        .expect("exports")
}

#[test]
fn the_export_is_deterministic_and_options_are_honoured() {
    let (db, header) = (sample_drawing(), sample_header());
    let options = ExportOptions {
        max_levels: 1,
        svg: true,
        full: true,
        shard_kb: 4,
        source_name: Some("sample.dxf".into()),
        ..Default::default()
    };
    let a = TempDir::new("det_a");
    let b = TempDir::new("det_b");
    let ra = iron_pack_cad::export_package(&db, Some(&header), &a.0, &options).expect("exports");
    let rb = iron_pack_cad::export_package(&db, Some(&header), &b.0, &options).expect("exports");
    assert!(a.0.join("drawing.svg").exists() && a.0.join("entities.json").exists());
    // A 4 KB shard size splits the geometry records.
    assert!(a.0.join("geometry.001.json").exists(), "{:?}", ra.files);
    let manifest = read_json(&a.0.join("manifest.json"));
    assert_eq!(manifest["source"]["name"], "sample.dxf");
    assert!(manifest["shard_index"].as_array().unwrap().len() > 5);
    // drawing.svg reads in world units for a drawing near the origin.
    assert_eq!(manifest["svg_origin"], serde_json::json!([0.0, 0.0]));

    // Same input, same bytes -- every file, report.json included: it holds
    // no clock.
    for file in &ra.files {
        let fa = std::fs::read(a.0.join(&file.path)).unwrap();
        let fb = std::fs::read(b.0.join(&file.path)).unwrap();
        assert!(fa == fb, "{} differs between runs", file.path);
    }
    assert_eq!(ra.counts, rb.counts);

    // Another profile changes the lattice.
    let c = TempDir::new("openai");
    let rc = export_sample(
        &c.0,
        &ExportOptions {
            profile: Profile::OPENAI_PATCH,
            max_levels: 0,
            ..Default::default()
        },
    );
    assert_eq!(rc.overview.px[0] % 32, 0);
}
