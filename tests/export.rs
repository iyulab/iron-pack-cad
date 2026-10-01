//! The package on the fixtures and on built drawings: every file it lists,
//! an overview sized to the profile, records with the exact numbers that
//! point at what the picture shows, and byte-identical output on a second
//! run.

mod common;

use std::collections::BTreeSet;

use common::*;
use iron_pack_cad::{ExportError, ExportOptions};
use serde_json::Value;
use uncad_model::model::{Entity, InsertEntity, LwPolylineEntity, Point2D, PolylineVertex, Ref};
use uncad_model::CadDatabase;

fn export_sample(dir: &std::path::Path, options: &ExportOptions) -> iron_pack_cad::ExportReport {
    iron_pack_cad::export_package(&sample_drawing(), Some(&sample_header()), dir, options)
        .expect("exports")
}

#[test]
fn the_package_has_every_file_and_a_profile_sized_overview() {
    let tmp = TempDir::new("files");
    let report = export_sample(&tmp.0, &ExportOptions::default());

    for name in [
        "README.txt",
        "manifest.json",
        "drawing.json",
        "overview.png",
        "blocks.json",
        "strings.json",
        "report.json",
    ] {
        assert!(tmp.0.join(name).exists(), "{name} is written");
    }
    // Record files may be sharded (`name.001.json`, ...).
    for name in ["texts", "dimensions", "geometry", "regions"] {
        assert!(
            tmp.0.join(format!("{name}.json")).exists()
                || tmp.0.join(format!("{name}.001.json")).exists(),
            "{name} is written"
        );
    }
    assert!(!tmp.0.join("drawing.svg").exists());
    assert!(!tmp.0.join("entities.json").exists());

    // The overview fits Claude's budget: edge <= 1568, patches <= 1568.
    let [w, h] = report.overview.px;
    assert!(w <= 1568 && h <= 1568, "{w}x{h}");
    assert!((w / 28) * (h / 28) <= 1568, "{w}x{h}");
    assert_eq!(w % 28, 0);
    assert_eq!(h % 28, 0);
    // Either the edge or the patch budget is the binding limit, up to the
    // lattice snap of the window, which may give up one patch per side:
    // two more patches on each side would break a limit.
    assert!(
        w.max(h) + 2 * 28 > 1568 || (w / 28 + 2) * (h / 28 + 2) > 1568,
        "the budget is used: {w}x{h}"
    );
    let png = std::fs::read(tmp.0.join("overview.png")).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(
        png_size(&png),
        [w, h],
        "the file is the size the report says"
    );

    let manifest = read_json(&tmp.0.join("manifest.json"));
    assert_eq!(manifest["$schema"], "iron-pack-cad/1");
    assert_eq!(manifest["profile"], "claude");
    assert_eq!(manifest["source"]["name"], Value::Null);
    assert_eq!(manifest["source"]["format"], "dxf");
    assert_eq!(manifest["svg_origin"], Value::Null, "no drawing.svg");
    assert_eq!(manifest["units"]["name"], "mm");
    assert_eq!(manifest["crop"]["source"], "content");
    assert!(manifest["guidance"]
        .as_str()
        .unwrap()
        .contains("strings.json"));
    assert!(manifest["files"].as_array().unwrap().len() == report.files.len());
    assert_eq!(manifest["counts"]["dimensions"], 0);
    assert_eq!(manifest["capabilities"]["fonts"], "bundled");
    // Every kind of record the drawing holds is there.
    assert_eq!(
        records(&tmp.0, "texts").len(),
        6 + 1 + 12,
        "texts, the MTEXT, the ATTRIBs"
    );
    assert_eq!(records(&tmp.0, "regions").len(), 3);
    assert_eq!(records(&tmp.0, "blocks").len(), 12);

    // Every listed file exists with the listed size (manifest, README and
    // report are listed without one: they are written last).
    for file in manifest["files"].as_array().unwrap() {
        let path = tmp.0.join(file["path"].as_str().unwrap());
        let meta = std::fs::metadata(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        if let Some(bytes) = file["bytes"].as_u64() {
            assert_eq!(meta.len(), bytes, "{}", path.display());
        } else {
            assert!(
                ["manifest.json", "README.txt", "report.json"]
                    .contains(&file["path"].as_str().unwrap()),
                "{file}"
            );
        }
    }
}

#[test]
fn the_units_are_the_ones_the_model_states_with_or_without_a_header() {
    let manifest_units = |db: &CadDatabase, header: Option<&iron_pack_cad::Header>, name: &str| {
        let tmp = TempDir::new(name);
        iron_pack_cad::export_package(db, header, &tmp.0, &ExportOptions::default())
            .expect("exports");
        let text = std::fs::read_to_string(tmp.0.join("manifest.json")).unwrap();
        serde_json::from_str::<Value>(&text).unwrap()["units"].clone()
    };

    // No header handed in: the model's own unit still stands.
    let units = manifest_units(&sample_drawing(), None, "units_no_header");
    assert_eq!(units["name"], "mm");
    assert_eq!(units["insunits"], 4);
    assert_eq!(units["source"], "header");

    // A header stating another code does not override the model's.
    let header: iron_pack_cad::Header =
        serde_json::from_value(serde_json::json!({ "insunits": 1 })).unwrap();
    let units = manifest_units(&sample_drawing(), Some(&header), "units_header");
    assert_eq!(units["name"], "mm");

    // A model stating none is drawing units, not a guess.
    let mut db = sample_drawing();
    db.header.insunits = None;
    let units = manifest_units(&db, Some(&sample_header()), "units_none");
    assert_eq!(units["name"], "du");
    assert_eq!(units["to_mm"], Value::Null);
    assert_eq!(units["source"], "none");
}

#[test]
fn hidden_entities_stay_out_of_the_records() {
    let tmp = TempDir::new("hidden");
    let report = export_fixture("hidden_layers_r2000.dxf", &tmp.0, &ExportOptions::default())
        .expect("exports");
    // Off, frozen, not plotted (NOPLOT and Defpoints both state DXF 290 =
    // 0) and one invisible LINE: five of the nine.
    assert_eq!(report.counts.hidden, 5);
    let geometry = records(&tmp.0, "geometry");
    assert_eq!(geometry.len(), 4, "{geometry:?}");
    let layers: BTreeSet<&str> = geometry
        .iter()
        .map(|g| g["layer"].as_str().unwrap())
        .collect();
    assert!(
        !layers.contains("OFF")
            && !layers.contains("FROZEN")
            && !layers.contains("NOPLOT")
            && !layers.contains("Defpoints"),
        "{layers:?}"
    );
    let dashed = geometry.iter().find(|g| g["layer"] == "VISIBLE").unwrap();
    assert_eq!(dashed["length"], 100.0);
    assert_eq!(dashed["unit"], "mm");
    assert!(records(&tmp.0, "texts").is_empty());
    let report_json = read_json(&tmp.0.join("report.json"));
    assert_eq!(report_json["hidden"]["by_reason"]["layer_off"], 1);
    assert_eq!(report_json["hidden"]["by_reason"]["layer_no_plot"], 1);
    assert_eq!(report_json["hidden"]["by_reason"]["defpoints"], 1);
    assert_eq!(report_json["hidden"]["by_reason"]["invisible"], 1);
}

#[test]
fn hidden_entities_are_counted_the_same_way_in_both_files() {
    let tmp = TempDir::new("hidden_agree");
    export_fixture(
        "hidden_layers_r2000.dxf",
        &tmp.0,
        &ExportOptions {
            max_levels: 1,
            ..Default::default()
        },
    )
    .expect("exports");
    let manifest = read_json(&tmp.0.join("manifest.json"));
    let report = read_json(&tmp.0.join("report.json"));
    let hidden = &report["hidden"];
    assert_eq!(
        hidden["count"].as_u64().unwrap(),
        manifest["counts"]["hidden"].as_u64().unwrap()
    );
    assert_eq!(
        hidden["top_level"].as_u64().unwrap() + hidden["inside_blocks"].as_u64().unwrap(),
        hidden["count"].as_u64().unwrap()
    );
    let by_reason: u64 = hidden["by_reason"]
        .as_object()
        .unwrap()
        .values()
        .map(|v| v.as_u64().unwrap())
        .sum();
    assert_eq!(by_reason, hidden["top_level"].as_u64().unwrap());
    assert!(by_reason > 0);
    assert_eq!(hidden["covers"], "top_level");
    assert_eq!(
        hidden["handles"].as_array().unwrap().len(),
        hidden["top_level"].as_u64().unwrap() as usize
    );
    assert_eq!(
        hidden["ids"].as_array().unwrap().len(),
        hidden["top_level"].as_u64().unwrap() as usize
    );
    assert_eq!(hidden["handles_truncated"], false);
    assert_eq!(hidden["handles_limit"], 100);
}

/// One line and one TEXT saying `s`, in model space.
fn drawing_with_text(s: &str) -> CadDatabase {
    model_space(vec![
        line(0x10, 0.0, 0.0, 100.0, 0.0),
        text(0x11, 10.0, 10.0, 5.0, s),
    ])
}

#[test]
fn text_boxes_are_measured_with_the_bundled_font_and_gaps_are_reported() {
    // Hangul, CP949-decoded, shaped by the bundled Noto Sans KR subset.
    let tmp = TempDir::new("hangul");
    let report = export_fixture(
        "cp949_r2000.dxf",
        &tmp.0,
        &ExportOptions {
            max_levels: 0,
            ..Default::default()
        },
    )
    .expect("exports");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let texts = records(&tmp.0, "texts");
    assert_eq!(texts.len(), 5);
    assert!(texts
        .iter()
        .all(|t| t["bbox_confidence"] == "measured" && t["font_ok"] == true));
    // The two syllables at height 2.5 are drawn at font-size 2.5 / 0.733 = 3.41 (the CAD
    // height is the cap height): two syllables of roughly 0.85 em each, so
    // about 5.8 units wide, and a Hangul glyph is about 0.88 em tall, so
    // about 3 units -- taller than the 2.5 of a capital.
    // The fixture states no handles: the record is found by its text.
    let domyeon = texts
        .iter()
        .find(|t| t["text"] == "\u{b3c4}\u{ba74}")
        .unwrap_or_else(|| panic!("the text of two syllables: {texts:?}"));
    assert_eq!(domyeon["text"], "\u{b3c4}\u{ba74}");
    let b = domyeon["bbox"].as_array().unwrap();
    let (w, h) = (
        b[2].as_f64().unwrap() - b[0].as_f64().unwrap(),
        b[3].as_f64().unwrap() - b[1].as_f64().unwrap(),
    );
    assert!(
        (4.8..7.5).contains(&w) && (2.5..3.8).contains(&h),
        "{w} x {h}"
    );
    let manifest = read_json(&tmp.0.join("manifest.json"));
    assert_eq!(manifest["capabilities"]["text_boxes"], "measured");

    // Hanja is outside the subset: the box is measured (as .notdef boxes),
    // the record says the font failed, and the manifest warns.
    let tmp = TempDir::new("hanja");
    let report = export_db(
        &drawing_with_text("\u{6f22}\u{5b57} A"),
        &tmp.0,
        &ExportOptions {
            max_levels: 0,
            ..Default::default()
        },
    )
    .expect("exports");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.starts_with("UnshapedGlyphs")),
        "{:?}",
        report.warnings
    );
    let texts = records(&tmp.0, "texts");
    let t = by_handle(&texts, "11");
    assert_eq!(t["id"], "17");
    assert_eq!(t["font_ok"], false);
    assert_eq!(t["unshaped_glyphs"], 2);
    assert_eq!(t["bbox_confidence"], "measured");
}

#[test]
fn a_very_wide_drawing_raises_the_tiny_overview_warning() {
    // Two 1000-unit lines 2 units apart with a label between them: a 500:1
    // drawing. The fit for 1000 x 2 content is 56 patches wide and one
    // patch (28 px) tall: far under 200 on the short edge.
    let db = model_space(vec![
        line(0x10, 0.0, 0.0, 1000.0, 0.0),
        line(0x11, 0.0, 2.0, 1000.0, 2.0),
        text(0x12, 10.0, 0.5, 1.0, "WIDE"),
    ]);
    let tmp = TempDir::new("wide");
    let report = export_db(
        &db,
        &tmp.0,
        &ExportOptions {
            max_levels: 0,
            ..Default::default()
        },
    )
    .expect("exports");
    let [w, h] = report.overview.px;
    assert_eq!(h, 28, "{w}x{h}");
    assert!(w >= 200, "{w}x{h}");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.starts_with("TinyOverview")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn capitals_are_drawn_as_tall_as_the_cad_text_height() {
    // A capital-only TEXT of height 2 and a three-line capital-only MTEXT
    // of height 2.5 attached at its top-left corner, over a line.
    let db = model_space(vec![
        line(0x10, 0.0, 0.0, 100.0, 0.0),
        text(0x11, 10.0, 10.0, 2.0, "HIH"),
        mtext(0x12, 10.0, 40.0, 2.5, r"H\PH\PH"),
    ]);
    let tmp = TempDir::new("capitals");
    export_db(
        &db,
        &tmp.0,
        &ExportOptions {
            max_levels: 0,
            ..Default::default()
        },
    )
    .expect("exports");
    let texts = records(&tmp.0, "texts");
    let bbox = |handle: &str| -> [f64; 4] {
        let t = by_handle(&texts, handle);
        assert_eq!(t["bbox_confidence"], "measured");
        let b = t["bbox"].as_array().unwrap();
        [
            b[0].as_f64().unwrap(),
            b[1].as_f64().unwrap(),
            b[2].as_f64().unwrap(),
            b[3].as_f64().unwrap(),
        ]
    };
    // The CAD height is the height of the capitals: "HIH" at height 2
    // measures 2 units from baseline to cap top, sitting on its baseline at
    // y = 10.
    let t = bbox("11");
    assert!((t[3] - t[1] - 2.0).abs() < 0.05, "{t:?}");
    assert!((t[1] - 10.0).abs() < 0.05, "{t:?}");
    assert_eq!(by_handle(&texts, "11")["height"], 2.0);
    // Three lines of height 2.5 at AutoCAD's 5/3 spacing: the first cap top
    // at the insertion point (top attachment), baselines 4.1667 apart, so
    // the block runs from y = 40 down to the last baseline at 29.1667.
    let m = bbox("12");
    assert!((m[3] - 40.0).abs() < 0.05, "{m:?}");
    assert!(
        (m[3] - m[1] - 2.5 * (1.0 + 2.0 * 5.0 / 3.0)).abs() < 0.05,
        "{m:?}"
    );
    assert_eq!(by_handle(&texts, "12")["text"], "H\nH\nH");
    assert_eq!(by_handle(&texts, "12")["raw"], r"H\PH\PH");
}

#[test]
fn exporting_onto_an_existing_file_reports_the_path_it_could_not_write() {
    let tmp = TempDir::new("io");
    std::fs::create_dir_all(&tmp.0).expect("a writable directory");
    let file = tmp.0.join("not-a-directory");
    std::fs::write(&file, b"in the way").expect("writable");
    let err = export_db(&drawing_with_text("A"), &file, &ExportOptions::default())
        .expect_err("a file is not a directory");
    match &err {
        ExportError::Io { path, source } => {
            assert_eq!(path, &file);
            assert!(
                matches!(
                    source.kind(),
                    std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::NotADirectory
                ),
                "{source:?}"
            );
        }
        other => panic!("expected an Io error, got {other:?}"),
    }
    let message = err.to_string();
    assert!(
        message.starts_with(&format!("cannot write {}", file.display())),
        "{message}"
    );
    assert!(file.is_file());
}

#[test]
fn an_outline_too_big_to_test_says_so_instead_of_claiming_it_is_simple() {
    // Above the cap the answer is `null` -- not the `true` that would let a
    // reader trust an area that may be meaningless.
    for (n, want_simple) in [(64usize, true), (8000, false)] {
        let points: Vec<(f64, f64)> = (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                (100.0 * a.cos(), 100.0 * a.sin())
            })
            .collect();
        let db = model_space(vec![lwpolyline(0x10, &points, &[], true)]);
        let tmp = TempDir::new(&format!("ring{n}"));
        let started = std::time::Instant::now();
        export_db(
            &db,
            &tmp.0,
            &ExportOptions {
                max_levels: 1,
                ..Default::default()
            },
        )
        .expect("exports");
        let elapsed = started.elapsed();
        let regions = records(&tmp.0, "regions");
        assert_eq!(regions.len(), 1, "one closed polyline, one region");
        let region = &regions[0];
        assert_eq!(region["vertex_count"], n);
        if want_simple {
            assert_eq!(region["simple"], true);
            assert_eq!(region["confidence"], "exact");
        } else {
            assert!(region["simple"].is_null(), "{}", region["simple"]);
            assert_eq!(region["confidence"], "estimated");
            assert!(
                region["why"]
                    .as_str()
                    .is_some_and(|w| w.contains("self-intersection")),
                "{}",
                region["why"]
            );
            assert!(
                elapsed < std::time::Duration::from_secs(60),
                "{elapsed:?} for one polyline"
            );
        }
        assert!(region["area"].as_f64().is_some_and(|a| a > 0.0));
    }
}

#[test]
fn record_building_does_not_grow_with_the_square_of_the_entity_count() {
    // Four times the entities may cost at most `LIMIT` times the time: a
    // linear pass over every extent per record made it quadratic.
    const LIMIT: f64 = 5.5;
    let time_of = |count: usize| -> f64 {
        let entities: Vec<Entity> = (0..count)
            .map(|i| {
                let (x, y) = ((i % 500) as f64 * 2.0, (i / 500) as f64 * 2.0);
                line(0x1000 + i as u64, x, y, x + 1.0, y)
            })
            .collect();
        let db = model_space(entities);
        let tmp = TempDir::new(&format!("scale{count}"));
        let started = std::time::Instant::now();
        export_db(
            &db,
            &tmp.0,
            &ExportOptions {
                max_levels: 1,
                ..Default::default()
            },
        )
        .expect("exports");
        started.elapsed().as_secs_f64()
    };
    time_of(500);
    let small = time_of(12_500);
    let big = time_of(50_000);
    assert!(
        big <= small * LIMIT,
        "50 000 entities took {big:.2}s against {small:.2}s for 12 500 ({:.1}x)",
        big / small
    );
}

#[test]
fn a_lines_length_is_its_length_in_three_dimensions() {
    // The expected numbers are 3-4-5 triangles: (0,50,0)->(300,50,400) is
    // 300 across and 400 up, so 500; the vertical one is 0 across and 100
    // up; the flat one is 400 across and 0 up, and has no `dz` at all.
    let mut vertical = line(0xA, 0.0, 0.0, 0.0, 0.0);
    let mut rafter = line(0xB, 0.0, 50.0, 300.0, 50.0);
    if let Entity::Line(l) = &mut vertical {
        l.end_point.z = 100.0;
    }
    if let Entity::Line(l) = &mut rafter {
        l.end_point.z = 400.0;
    }
    let db = model_space(vec![vertical, rafter, line(0xC, 0.0, 100.0, 400.0, 100.0)]);
    let tmp = TempDir::new("line3d");
    export_db(&db, &tmp.0, &ExportOptions::default()).expect("exports");
    let geometry = records(&tmp.0, "geometry");

    let vertical = by_handle(&geometry, "A");
    assert_eq!(vertical["length"], 100.0);
    assert_eq!(vertical["length_plan"], 0.0);
    assert_eq!(vertical["dz"], 100.0);
    assert_eq!(vertical["confidence"], "exact");
    let rafter = by_handle(&geometry, "B");
    assert_eq!(rafter["length"], 500.0);
    assert_eq!(rafter["length_plan"], 300.0);
    assert_eq!(rafter["dz"], 400.0);
    let flat = by_handle(&geometry, "C");
    assert_eq!(flat["length"], 400.0);
    assert!(flat.get("length_plan").is_none(), "{flat}");
    assert!(flat.get("dz").is_none(), "{flat}");
}

#[test]
fn a_donut_reports_its_area_and_gets_a_region_record() {
    // Two bulges of 1 over a 100-unit chord are a circle of radius 50, so
    // the area is pi * 50^2 = 7853.98163397 and the perimeter is
    // 2 * pi * 50 = 314.15926536: both from the circle, not from uncad.
    let db = model_space(vec![lwpolyline(
        0xD,
        &[(0.0, 0.0), (100.0, 0.0)],
        &[1.0, 1.0],
        true,
    )]);
    let tmp = TempDir::new("donut");
    let report = export_db(&db, &tmp.0, &ExportOptions::default()).expect("exports");
    let geometry = records(&tmp.0, "geometry");
    let g = &geometry[0];
    assert_eq!(g["area"], 7853.98163397, "{g}");
    assert_eq!(g["perimeter"], 314.15926536, "{g}");
    assert_eq!(g["orientation"], "ccw", "{g}");
    assert_eq!(g["confidence"], "exact", "{g}");
    assert_eq!(report.counts.regions, 1);
    let regions = records(&tmp.0, "regions");
    assert_eq!(regions[0]["area"], 7853.98163397, "{:?}", regions[0]);
    assert_eq!(regions[0]["centroid"], serde_json::json!([50.0, 0.0]));
    assert_eq!(regions[0]["vertex_count"], 2);
}

#[test]
fn a_mirrored_polyline_is_reported_where_the_picture_draws_it() {
    // The model keeps a planar entity in its own plane: this closed
    // polyline states (10,0)-(20,0)-(20,10) with the normal (0,0,-1), which
    // puts it at x -10..-20 in the world and runs it clockwise there. The
    // record is the world's -- the box the picture draws it in -- with its
    // arc bulge turned by the mirror and the same area.
    let mut mirrored = lwpolyline(
        0xE,
        &[(10.0, 0.0), (20.0, 0.0), (20.0, 10.0)],
        &[0.0, 0.5, 0.0],
        true,
    );
    if let Entity::LwPolyline(p) = &mut mirrored {
        p.extrusion.z = -1.0;
    }
    let plain = lwpolyline(
        0xF,
        &[(10.0, 0.0), (20.0, 0.0), (20.0, 10.0)],
        &[0.0, 0.5, 0.0],
        true,
    );
    let db = model_space(vec![mirrored, plain]);
    let tmp = TempDir::new("mirrored");
    export_db(&db, &tmp.0, &ExportOptions::default()).expect("exports");
    let geometry = records(&tmp.0, "geometry");
    let m = by_handle(&geometry, "E");
    let p = by_handle(&geometry, "F");
    assert_eq!(
        m["vertices"],
        serde_json::json!([[-10.0, 0.0, 0.0], [-20.0, 0.0, -0.5], [-20.0, 10.0, 0.0]])
    );
    assert_eq!(m["area"], p["area"]);
    assert_eq!(m["perimeter"], p["perimeter"]);
    assert_eq!(
        (m["orientation"].as_str(), p["orientation"].as_str()),
        (Some("cw"), Some("ccw"))
    );
    let b = m["bbox"].as_array().unwrap();
    assert!(
        b[0].as_f64().unwrap() <= -20.0 && b[2].as_f64().unwrap() <= -10.0,
        "{b:?}"
    );
}

#[test]
fn a_region_says_why_its_area_is_unavailable_and_the_manifest_says_how_many() {
    // Rectangle (0,0) (100,0) (100,50) (0,50): 5000. Bowtie (0,0) (100,0)
    // (20,60) (80,80), whose edges cross: the shoelace half-sum is 1400.
    let db = model_space(vec![
        lwpolyline(
            0x10,
            &[(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)],
            &[],
            true,
        ),
        lwpolyline(
            0x11,
            &[(0.0, 0.0), (100.0, 0.0), (20.0, 60.0), (80.0, 80.0)],
            &[],
            true,
        ),
    ]);
    let tmp = TempDir::new("areas");
    export_db(
        &db,
        &tmp.0,
        &ExportOptions {
            max_levels: 1,
            ..Default::default()
        },
    )
    .expect("exports");
    let regions = records(&tmp.0, "regions");
    assert_eq!(regions.len(), 2);
    let good = by_handle(&regions, "10");
    assert_eq!(good["confidence"], "exact");
    assert_eq!(good["area"].as_f64().unwrap(), 5000.0);
    assert!(good["why"].is_null());
    let bad = by_handle(&regions, "11");
    assert_eq!(bad["confidence"], "unavailable");
    assert_eq!(bad["simple"], false);
    assert_eq!(bad["area"].as_f64().unwrap(), 1400.0);
    assert!(
        bad["why"]
            .as_str()
            .is_some_and(|w| w.contains("self-intersecting")),
        "{bad}"
    );
    let geometry = records(&tmp.0, "geometry");
    assert_eq!(by_handle(&geometry, "11")["why"], bad["why"]);
    let manifest = read_json(&tmp.0.join("manifest.json"));
    assert_eq!(manifest["capabilities"]["areas"], "mixed");
    assert_eq!(
        manifest["capabilities"]["areas_by_confidence"],
        serde_json::json!({"exact": 1, "estimated": 0, "unavailable": 1})
    );
}

/// `levels` blocks nested one inside the next, the innermost holding a
/// TEXT, with one INSERT of the outermost in model space.
fn nested_blocks(levels: usize) -> CadDatabase {
    let insert = |id: u64, block: String| {
        Entity::Insert(InsertEntity {
            common: common(id),
            block_name: Ref::Resolved(block),
            insertion_point: p3(0.0, 0.0),
            scale: uncad_model::Point3D {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            rotation: 0.0,
            attribs: Vec::new(),
            extrusion: z_axis(),
        })
    };
    let mut blocks = Vec::new();
    let names: Vec<String> = (0..levels).map(|i| format!("L{i}")).collect();
    for (i, name) in names.iter().enumerate() {
        let child = if i + 1 == levels {
            text(0x200 + i as u64, 1.0, 1.0, 2.5, "DEEPTEXT")
        } else {
            insert(0x200 + i as u64, format!("L{}", i + 1))
        };
        blocks.push((name.as_str(), vec![child]));
    }
    with_blocks(
        vec![
            insert(0x100, "L0".into()),
            line(0x101, 0.0, 0.0, 40.0, 20.0),
        ],
        blocks,
    )
}

#[test]
fn a_text_twelve_blocks_deep_is_indexed_by_the_renderers_rule() {
    // The renderer follows block references 20 deep, and the records hold
    // every text it drew: twelve levels is a bound XREF of an assembly of
    // assemblies, deeper than the feature branch's first cap of 8.
    let db = nested_blocks(12);
    let tmp = TempDir::new("deep_blocks");
    export_db(
        &db,
        &tmp.0,
        &ExportOptions {
            max_levels: 1,
            ..Default::default()
        },
    )
    .expect("exports");
    let texts = records(&tmp.0, "texts");
    assert_eq!(texts.len(), 1, "{texts:?}");
    assert_eq!(texts[0]["text"], "DEEPTEXT");
    // The id is the chain of INSERT IDs -- the top-level one and the
    // eleven nested ones -- ending in the text's own.
    let id = texts[0]["id"].as_str().unwrap();
    assert_eq!(id.split('/').count(), 13, "{id}");
    assert!(id.starts_with("256/512/"), "{id}");
    let strings = read_json(&tmp.0.join("strings.json"));
    assert_eq!(strings["strings"]["deeptext"][0], id);
}

#[test]
fn every_record_id_resolves_to_its_file_through_the_shard_index() {
    // `first_key` and `last_key` publish the order of the ids, which are
    // strings a consumer would otherwise compare as strings; block
    // instances are in the index like every other kind.
    let tmp = TempDir::new("shard_lookup");
    export_sample(
        &tmp.0,
        &ExportOptions {
            shard_kb: 1,
            max_levels: 1,
            ..Default::default()
        },
    );
    let manifest = read_json(&tmp.0.join("manifest.json"));
    let index = manifest["shard_index"].as_array().unwrap();
    assert!(
        index.iter().filter(|s| s["kind"] == "blocks").count() > 1,
        "1 KB shards split twelve INSERT records"
    );
    let mut checked = 0;
    for (kind, name) in [
        ("text", "texts"),
        ("geometry", "geometry"),
        ("region", "regions"),
        ("blocks", "blocks"),
    ] {
        for record in records(&tmp.0, name) {
            let id = record["id"].as_str().unwrap();
            let key: u64 = id.split('/').next().unwrap().parse().unwrap();
            let found: Vec<&str> = index
                .iter()
                .filter(|s| {
                    s["kind"] == kind
                        && s["first_key"].as_u64().unwrap() <= key
                        && key <= s["last_key"].as_u64().unwrap()
                })
                .map(|s| s["file"].as_str().unwrap())
                .collect();
            assert_eq!(found.len(), 1, "{id} of kind {kind} resolved to {found:?}");
            let shard = read_json(&tmp.0.join(found[0]));
            assert!(
                shard["records"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == id),
                "{id} is not in {}",
                found[0]
            );
            checked += 1;
        }
    }
    assert!(checked > 50, "{checked} records");
}

#[test]
fn a_nested_block_references_attribute_is_indexed_once() {
    // The tag-inside-assembly pattern: block DOOR holds an INSERT of block
    // TAG whose ATTRIB says NUM = D-101. DOOR is inserted at (100, 100)
    // and the ATTRIB sits at (21, 21) in DOOR's own frame, so the value is
    // drawn at (121, 121) at height 2.5; its id is the INSERT's and the
    // ATTRIB's, the path the renderer names the <text> with.
    let tmp = TempDir::new("nested_attrib");
    export_fixture(
        "nested_attrib_r2000.dxf",
        &tmp.0,
        &ExportOptions {
            max_levels: 1,
            svg: true,
            ..Default::default()
        },
    )
    .expect("exports");
    let texts = records(&tmp.0, "texts");
    let nested = texts
        .iter()
        .find(|t| t["text"] == "D-101")
        .unwrap_or_else(|| panic!("the nested attribute: {texts:?}"));
    assert_eq!(nested["id"], format!("{}/{}", 0x60, 0x56));
    assert_eq!(nested["kind"], "ATTRIB");
    assert_eq!(nested["tag"], "NUM");
    let b: Vec<f64> = nested["bbox"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert!(
        (120.5..122.5).contains(&b[0]) && (120.5..122.5).contains(&b[1]),
        "{b:?}"
    );
    assert!((b[3] - b[1] - 2.5).abs() < 0.3, "cap height 2.5: {b:?}");
    assert_eq!(texts.iter().filter(|t| t["text"] == "D-TOP").count(), 1);
    assert_eq!(texts.iter().filter(|t| t["text"] == "D-101").count(), 1);
    let strings = read_json(&tmp.0.join("strings.json"));
    assert_eq!(
        strings["strings"]["d-101"],
        serde_json::json!([nested["id"]]),
        "{}",
        strings["strings"]
    );
    let svg = std::fs::read_to_string(tmp.0.join("drawing.svg")).unwrap();
    assert_eq!(svg.matches("D-101").count(), 1, "drawn once");
    assert!(svg.contains(&format!("id=\"t{}.{}\"", 0x60, 0x56)));
}

#[test]
fn a_nested_attribute_stored_on_the_insert_is_indexed_too() {
    // The same nesting in the shape a DWG (and an R2004+ DXF) gives: the
    // nested INSERT carries its ATTRIB in `attribs` and the block has no
    // ATTRIB child at all.
    let value = attrib(0x56, 21.0, 21.0, "NUM", "D-101");
    let db = with_blocks(
        vec![insert(0x60, "DOOR", 100.0, 100.0, Vec::new())],
        vec![
            ("TAG", vec![line(0x42, 0.0, 0.0, 10.0, 0.0)]),
            (
                "DOOR",
                vec![
                    line(0x52, 0.0, 0.0, 40.0, 0.0),
                    line(0x53, 0.0, 0.0, 0.0, 40.0),
                    insert(0x55, "TAG", 20.0, 20.0, vec![value]),
                ],
            ),
        ],
    );
    let tmp = TempDir::new("nested_attrib_dwg");
    export_db(
        &db,
        &tmp.0,
        &ExportOptions {
            max_levels: 0,
            svg: true,
            ..Default::default()
        },
    )
    .expect("exports");
    let texts = records(&tmp.0, "texts");
    assert_eq!(texts.len(), 1, "{texts:?}");
    assert_eq!(texts[0]["id"], format!("{}/{}", 0x60, 0x56));
    assert_eq!(texts[0]["text"], "D-101");
    let b: Vec<f64> = texts[0]["bbox"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert!(
        (120.5..122.5).contains(&b[0]) && (120.5..122.5).contains(&b[1]),
        "{b:?}"
    );
    let svg = std::fs::read_to_string(tmp.0.join("drawing.svg")).unwrap();
    assert_eq!(svg.matches("D-101").count(), 1, "drawn once: {svg}");
}

#[test]
fn a_mirrored_block_places_its_text_where_the_picture_draws_it() {
    // A block holding one text at (10, 0), inserted at (100, 100) with the
    // normal (0, 0, -1): the block is mirrored across the world's y axis,
    // so its text lands at x = -100 - 10 = -110 and reads as mirror
    // writing, not turned: rotation 0.
    let mut mirrored = Entity::Insert(InsertEntity {
        common: common(0x70),
        block_name: Ref::Resolved("B".into()),
        insertion_point: p3(100.0, 100.0),
        scale: uncad_model::Point3D {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        rotation: 0.0,
        attribs: Vec::new(),
        extrusion: z_axis(),
    });
    if let Entity::Insert(i) = &mut mirrored {
        i.extrusion.z = -1.0;
    }
    let db = with_blocks(
        vec![mirrored, line(0x71, -200.0, 0.0, 0.0, 200.0)],
        vec![("B", vec![text(0x72, 10.0, 0.0, 2.0, "MIRROR")])],
    );
    let tmp = TempDir::new("mirrored_block");
    export_db(&db, &tmp.0, &ExportOptions::default()).expect("exports");
    let texts = records(&tmp.0, "texts");
    assert_eq!(texts.len(), 1, "{texts:?}");
    let t = &texts[0];
    assert_eq!(t["anchor"], serde_json::json!([-110.0, 100.0]));
    assert_eq!(t["rotation_deg"], 0.0);
    assert_eq!(t["height"], 2.0);
    let blocks = records(&tmp.0, "blocks");
    assert_eq!(blocks[0]["mirrored"], true);
    assert_eq!(blocks[0]["at"], serde_json::json!([-100.0, 100.0]));
    // Its box is on the mirrored side, and the picture's.
    let b: Vec<f64> = t["bbox"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert!(b[2] <= -109.0 && b[0] < -115.0, "{b:?}");
}

#[test]
fn a_record_is_built_from_a_polyline_whose_first_vertex_repeats() {
    // A closed outline whose file repeats the first vertex: the repeat adds
    // a segment of no length, and the outline has the area of the square.
    let db = model_space(vec![Entity::LwPolyline(LwPolylineEntity {
        common: common(0x30),
        vertices: [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (0.0, 0.0)]
            .iter()
            .map(|(x, y)| PolylineVertex::straight(Point2D { x: *x, y: *y }))
            .collect(),
        closed: true,
        const_width: 0.0,
        elevation: 0.0,
        extrusion: z_axis(),
    })]);
    let tmp = TempDir::new("repeat");
    export_db(&db, &tmp.0, &ExportOptions::default()).expect("exports");
    let regions = records(&tmp.0, "regions");
    assert_eq!(regions[0]["area"], 16.0);
    assert_eq!(regions[0]["perimeter"], 16.0);
    assert_eq!(regions[0]["simple"], true);
}

/// A raster image and the definition naming its file: the image is drawn as
/// its outline, so it measures an extent and gets a geometry record that
/// says where it sits and which file it shows.
#[test]
fn an_image_is_a_geometry_record_naming_its_file() {
    use uncad_model::model::ImageEntity;
    use uncad_model::tables::ImageDefinition;
    let image = Entity::Image(ImageEntity {
        common: common(0x20),
        insertion_point: p3(10.0, 5.0),
        u_vector: p3(0.5, 0.0),
        v_vector: uncad_model::Point3D {
            x: 0.0,
            y: 0.5,
            z: 0.0,
        },
        size_pixels: Point2D { x: 40.0, y: 20.0 },
        definition: Ref::Resolved("80F".to_string()),
        display_flags: Some(7),
        clipping: Some(false),
        brightness: Some(50),
        contrast: Some(50),
        fade: Some(0),
        clip_outside: None,
        boundary: Vec::new(),
    });
    let mut db = model_space(vec![line(0x10, 0.0, 0.0, 100.0, 0.0), image]);
    db.tables.image_definitions.insert(
        "80F".to_string(),
        ImageDefinition {
            file_path: Some("images/plan.png".to_string()),
            size_pixels: Point2D { x: 40.0, y: 20.0 },
            pixel_size: Point2D { x: 0.1, y: 0.1 },
            loaded: Some(true),
            resolution_unit: None,
        },
    );
    let tmp = TempDir::new("image");
    export_db(&db, &tmp.0, &ExportOptions::default()).expect("exports");
    let geometry = records(&tmp.0, "geometry");
    let image = geometry
        .iter()
        .find(|r| r["type"] == "IMAGE")
        .expect("the image has a record");
    assert_eq!(image["file_path"], "images/plan.png");
    assert_eq!(image["size_px"], serde_json::json!([40.0, 20.0]));
    assert_eq!(image["clipping"], false);
    assert_eq!(image["confidence"], "exact");
    // The whole frame: 20 x 10 drawing units from (10, 5).
    let bbox = &image["bbox"];
    assert_eq!(bbox, &serde_json::json!([10.0, 5.0, 30.0, 15.0]), "{image}");
    let report = read_json(&tmp.0.join("report.json"));
    assert!(
        !report["unsupported_types"]
            .as_array()
            .is_some_and(|t| t.iter().any(|n| n == "IMAGE")),
        "{report}"
    );
}

/// A geometry record that is not `exact` always says why: the hatch of a
/// real file, and every type the package measures nothing of.
#[test]
fn every_geometry_record_that_is_not_exact_says_why() {
    let tmp = TempDir::new("why");
    export_fixture(
        "hatched_viewport_r2000.dxf",
        &tmp.0,
        &ExportOptions::default(),
    )
    .expect("exports");
    let geometry = records(&tmp.0, "geometry");
    assert!(
        geometry.iter().any(|r| r["type"] == "HATCH"),
        "the fixture has a hatch"
    );
    for r in &geometry {
        if r["confidence"] != "exact" {
            assert!(r["why"].is_string(), "{r}");
        }
    }
}

#[test]
fn an_arc_whose_angles_are_equal_is_named_in_the_report_not_measured() {
    use uncad_model::model::ArcEntity;
    let arc = |id: u64, start: f64, end: f64| {
        Entity::Arc(ArcEntity {
            common: common(id),
            center: p3(0.0, 0.0),
            radius: 10.0,
            start_angle: start,
            end_angle: end,
            extrusion: z_axis(),
        })
    };
    // The quarter from 0 to 90 degrees is measured; the arc whose angles are
    // equal is neither the whole circle nor nothing as far as the file says.
    let db = model_space(vec![
        arc(0x40, 1.0, 1.0),
        arc(0x41, 0.0, std::f64::consts::FRAC_PI_2),
    ]);
    let tmp = TempDir::new("arc-equal");
    export_db(&db, &tmp.0, &ExportOptions::default()).expect("exports");
    let geometry = records(&tmp.0, "geometry");
    assert_eq!(geometry.len(), 1, "{geometry:?}");
    let quarter = by_handle(&geometry, "41");
    assert_eq!(quarter["sweep_deg"], 90.0, "{quarter}");
    let report = read_json(&tmp.0.join("report.json"));
    assert_eq!(report["undefined_arcs"], serde_json::json!([0x40]));
}

#[test]
fn an_ellipse_whose_parameters_are_equal_is_the_whole_ellipse() {
    use uncad_model::model::EllipseEntity;
    // Semi-axes 10 and 5: pi * 10 * 5 = 157.079632679. The file states the
    // same parameter twice, away from zero.
    let db = model_space(vec![Entity::Ellipse(EllipseEntity {
        common: common(0x50),
        center: p3(0.0, 0.0),
        major_axis_endpoint: p3(10.0, 0.0),
        axis_ratio: 0.5,
        start_angle: 1.0,
        end_angle: 1.0,
        extrusion: z_axis(),
    })]);
    let tmp = TempDir::new("ellipse-equal");
    export_db(&db, &tmp.0, &ExportOptions::default()).expect("exports");
    let geometry = records(&tmp.0, "geometry");
    assert_eq!(geometry[0]["area"], 157.079632679, "{:?}", geometry[0]);
}
