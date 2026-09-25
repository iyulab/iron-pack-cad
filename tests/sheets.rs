//! Sheets: one image per paper layout, the model composited through its
//! viewports by the renderer, the sheet placed by the layout's limits or
//! its page setup, what each viewport shows and how a model point maps onto
//! the paper, and the sheet's own texts as records on it. Sources: the
//! fixtures (see tests/fixtures/README.md) -- a viewport twisted 30 degrees
//! at scale 2, a page setup with a plot origin, one viewport per state, a
//! title block -- and drawings built here.
//!
//! What a sheet image *shows* -- the model clipped to each viewport's
//! frame, each viewport's hatch pattern, a construction line cut at the
//! frame -- is the renderer's to prove, and its tests do, on the numbers of
//! the SVG it writes. These tests hold the package to the numbers it
//! publishes: the sheet rectangle, the viewports' states and maps, the
//! records' boxes.

mod common;

use common::*;
use iron_pack_cad::ExportOptions;
use serde_json::Value;
use uncad_model::model::{Entity, PointEntity};
use uncad_model::tables::{BlockRecord, LayoutRecord, PlotSettings};
use uncad_model::{CadDatabase, Point2D, Point3D, Ref};

fn one_level() -> ExportOptions {
    ExportOptions {
        max_levels: 0,
        ..Default::default()
    }
}

#[test]
fn the_export_writes_one_sheet_per_paper_layout() {
    let tmp = TempDir::new("sheet");
    let report =
        export_fixture("twisted_viewport_r2000.dxf", &tmp.0, &one_level()).expect("exports");
    assert_eq!(report.sheets.len(), 1);
    assert_eq!(report.counts.sheets, 1);
    let s1 = &report.sheets[0];
    assert_eq!(
        (s1.name.as_str(), s1.tab_order, s1.units.as_str()),
        ("Layout1", 1, "mm")
    );
    assert_eq!(s1.overview.png, "sheets/Layout1/overview.png");
    let png = std::fs::read(tmp.0.join(&s1.overview.png)).expect("the sheet image");
    assert_eq!(png_size(&png), s1.overview.px);
    assert_eq!(s1.overview.px[0] % 28, 0);
    // The whole sheet, no padding: the image's world rectangle is the sheet
    // (grown only by the lattice snap).
    assert!((s1.overview.world.min_x - s1.rect.min_x).abs() < 1e-9);
    assert!((s1.overview.world.max_y - s1.rect.max_y).abs() < 1e-9);
    assert!(s1.overview.world.max_x >= s1.rect.max_x - 1e-9);
    assert!(s1.overview.world.min_y <= s1.rect.min_y + 1e-9);

    let sheets = read_json(&tmp.0.join("sheets.json"));
    assert_eq!(sheets["sheets"].as_array().unwrap().len(), 1);
    assert!(sheets["twist_convention"]
        .as_str()
        .unwrap()
        .contains("counter-clockwise"));
    let manifest = read_json(&tmp.0.join("manifest.json"));
    assert_eq!(manifest["sheets"].as_array().unwrap().len(), 1);
    assert_eq!(manifest["capabilities"]["paper_layouts"], "composited");
    assert_eq!(manifest["counts"]["sheets"], 1);

    // Opting out.
    let quiet = export_fixture(
        "twisted_viewport_r2000.dxf",
        &TempDir::new("nosheets").0,
        &ExportOptions {
            max_levels: 0,
            sheets: false,
            ..Default::default()
        },
    )
    .expect("exports");
    assert!(quiet.sheets.is_empty());
    assert!(!quiet.files.iter().any(|f| f.path.starts_with("sheets")));
}

#[test]
fn every_world_box_in_the_package_is_the_documented_array() {
    // "boxes are [x0, y0, x1, y1] in world units": the manifest's overview,
    // frames and crop, sheets.json, tiles.json and report.json alike.
    let tmp = TempDir::new("boxes");
    let (db, mut header) = fixture("twisted_viewport_r2000.dxf");
    // Extents the header states, so the crop reports them too.
    header.extmin = Some(Point3D {
        x: -10.0,
        y: -10.0,
        z: 0.0,
    });
    header.extmax = Some(Point3D {
        x: 110.0,
        y: 60.0,
        z: 0.0,
    });
    let report = iron_pack_cad::export_package(
        &db,
        Some(&header),
        &tmp.0,
        &ExportOptions {
            max_levels: 1,
            ..Default::default()
        },
    )
    .expect("exports");
    let manifest = read_json(&tmp.0.join("manifest.json"));
    let sheets = read_json(&tmp.0.join("sheets.json"));
    let tiles = read_json(&tmp.0.join("tiles.json"));
    let report_json = read_json(&tmp.0.join("report.json"));
    let box_of = |value: &Value, what: &str| -> [f64; 4] {
        let array = value
            .as_array()
            .unwrap_or_else(|| panic!("{what} is an array, got {value}"));
        assert_eq!(array.len(), 4, "{what}: {value}");
        let mut out = [0.0; 4];
        for (slot, v) in out.iter_mut().zip(array) {
            *slot = v.as_f64().unwrap_or_else(|| panic!("{what}: {value}"));
        }
        out
    };
    let overview = box_of(&manifest["overview"]["world"], "manifest.overview.world");
    let world = report.overview.world;
    for (got, want) in overview
        .iter()
        .zip([world.min_x, world.min_y, world.max_x, world.max_y])
    {
        assert!((got - want).abs() < 1e-9, "{got} vs {want}");
    }
    box_of(&manifest["crop"]["rect"], "manifest.crop.rect");
    box_of(&manifest["crop"]["content"], "manifest.crop.content");
    box_of(
        &manifest["crop"]["header_extents"],
        "manifest.crop.header_extents",
    );
    for excluded in manifest["crop"]["excluded"].as_array().unwrap() {
        box_of(&excluded["rect"], "manifest.crop.excluded[].rect");
    }
    for excluded in report_json["excluded"].as_array().unwrap() {
        if !excluded["rect"].is_null() {
            box_of(&excluded["rect"], "report.excluded[].rect");
        }
    }
    for frame in manifest["frames"].as_array().unwrap() {
        box_of(&frame["content"], "manifest.frames[].content");
        box_of(
            &frame["overview"]["world"],
            "manifest.frames[].overview.world",
        );
    }
    let content = box_of(&manifest["frames"][0]["content"], "frames[0].content");
    let rounded = box_of(&tiles["frames"][0]["content"], "tiles.frames[0].content");
    for (a, b) in content.iter().zip(rounded) {
        assert!((a - b).abs() < 1e-3, "{a} vs {b}");
    }
    for sheet in sheets["sheets"].as_array().unwrap() {
        box_of(&sheet["rect"], "sheets.json rect");
        box_of(&sheet["overview"]["world"], "sheets.json overview.world");
        for viewport in sheet["viewports"].as_array().unwrap() {
            box_of(&viewport["frame"], "sheets.json viewports[].frame");
        }
    }
    let from_manifest = box_of(&manifest["sheets"][0]["rect"], "manifest.sheets[0].rect");
    let from_sheets = box_of(&sheets["sheets"][0]["rect"], "sheets.json sheets[0].rect");
    for (a, b) in from_manifest.iter().zip(from_sheets) {
        assert!((a - b).abs() < 1e-3, "{a} vs {b}");
    }
}

/// Three paper layouts whose names all sanitise to the same directory
/// string: two three-syllable Hangul names and an empty one. Each has its
/// own paper block holding one line of its own length, and its own limits.
fn colliding_layout_names() -> CadDatabase {
    let mut db = model_space(vec![line(0x10, 10.0, 10.0, 100.0, 50.0)]);
    for (n, name) in ["\u{d3c9}\u{ba74}\u{b3c4}", "\u{c785}\u{ba74}\u{b3c4}", ""]
        .into_iter()
        .enumerate()
    {
        let block = if n == 0 {
            "*Paper_Space".to_string()
        } else {
            format!("*Paper_Space{}", n - 1)
        };
        let width = 100.0 + 40.0 * n as f64;
        let paper = vec![line(0x20 + n as u64, 10.0, 10.0, width - 10.0, 60.0)];
        db.entities.extend(paper.iter().cloned());
        db.tables.block_records.insert(
            block.clone(),
            BlockRecord {
                base_point: Default::default(),
                name: block.clone(),
                entities: paper,
            },
        );
        db.tables.layouts.insert(
            name.to_string(),
            LayoutRecord {
                name: name.to_string(),
                tab_order: n as i32 + 1,
                block_name: Ref::Resolved(block),
                limits_min: Point2D { x: 0.0, y: 0.0 },
                limits_max: Point2D { x: width, y: 80.0 },
                plot_settings: PlotSettings {
                    paper_name: String::new(),
                    paper_width: 0.0,
                    paper_height: 0.0,
                    margin_left: 0.0,
                    margin_bottom: 0.0,
                    margin_right: 0.0,
                    margin_top: 0.0,
                    plot_origin: Point2D::default(),
                    paper_units: None,
                    rotation: None,
                    scale_numerator: 0.0,
                    scale_denominator: 0.0,
                },
                paper_space_linetype_scaling: false,
                limits_check: false,
                extents_min: None,
                extents_max: None,
                active_viewport: Ref::Absent,
            },
        );
    }
    db
}

#[test]
fn layouts_whose_names_collide_get_a_sheet_image_each() {
    let tmp = TempDir::new("collide");
    let report = export_db(&colliding_layout_names(), &tmp.0, &one_level()).expect("exports");
    assert_eq!(report.sheets.len(), 3);
    let paths: Vec<&str> = report
        .sheets
        .iter()
        .map(|s| s.overview.png.as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "sheets/___/overview.png",
            "sheets/____2/overview.png",
            "sheets/sheet/overview.png"
        ],
        "the suffix goes to the later tab, the empty name to the placeholder"
    );
    let manifest = read_json(&tmp.0.join("manifest.json"));
    let files = manifest["files"].as_array().unwrap();
    let listed: Vec<&str> = files.iter().map(|f| f["path"].as_str().unwrap()).collect();
    let unique: std::collections::BTreeSet<&str> = listed.iter().copied().collect();
    assert_eq!(listed.len(), unique.len(), "{listed:?}");
    let mut images: Vec<Vec<u8>> = Vec::new();
    for sheet in &report.sheets {
        let bytes = std::fs::read(tmp.0.join(&sheet.overview.png)).expect("the sheet image");
        assert!(!images.contains(&bytes), "{} repeats", sheet.overview.png);
        images.push(bytes);
    }
    let sheets = read_json(&tmp.0.join("sheets.json"));
    let from_json: Vec<&str> = sheets["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["overview"]["png"].as_str().unwrap())
        .collect();
    assert_eq!(from_json, paths);
}

/// A composited viewport's published map, applied to a model point.
fn to_paper(m: &[f64; 6], x: f64, y: f64) -> [f64; 2] {
    [m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5]]
}

#[test]
fn the_model_is_composited_through_a_twisted_viewport() {
    let tmp = TempDir::new("twisted");
    let report =
        export_fixture("twisted_viewport_r2000.dxf", &tmp.0, &one_level()).expect("exports");
    assert_eq!(report.sheets.len(), 1, "{:?}", report.sheets);
    let sheet = &report.sheets[0];
    assert_eq!(sheet.viewports.len(), 1);
    let vp = &sheet.viewports[0];
    assert!(vp.composited && !vp.overall && vp.on);
    assert!((vp.scale.unwrap() - 2.0).abs() < 1e-9);
    assert!((vp.twist_deg - 30.0).abs() < 1e-9);
    // The frame's corners in the model (ezdxf's convention): the view
    // centre un-rotated by the twist, the window 100 x 60 model units.
    let window = vp.model_window.expect("a stored view");
    for (got, (x, y)) in window.iter().zip([
        (-2.5, -4.330),
        (84.103, -54.330),
        (114.103, -2.369),
        (27.5, 47.631),
    ]) {
        assert!(
            (got[0] - x).abs() < 1e-3 && (got[1] - y).abs() < 1e-3,
            "{got:?} vs ({x}, {y})"
        );
    }
    let m = vp.model_to_paper.expect("the map it was drawn through");
    // The model origin lands at (50, 50) on the paper, and (100, 50) at
    // (173.205, 236.603): scale 2, turned 30 degrees.
    let o = to_paper(&m, 0.0, 0.0);
    assert!(
        (o[0] - 50.0).abs() < 1e-9 && (o[1] - 50.0).abs() < 1e-9,
        "{o:?}"
    );
    let e = to_paper(&m, 100.0, 50.0);
    assert!(
        (e[0] - 173.205).abs() < 1e-3 && (e[1] - 236.603).abs() < 1e-3,
        "{e:?}"
    );
    // The window's corners land on the frame's, the same map applied (the
    // window is published to 1e-5, so 1e-4 on paper at scale 2).
    let frame = [
        [vp.frame.min_x, vp.frame.min_y],
        [vp.frame.max_x, vp.frame.min_y],
        [vp.frame.max_x, vp.frame.max_y],
        [vp.frame.min_x, vp.frame.max_y],
    ];
    for corner in window {
        let on_paper = to_paper(&m, corner[0], corner[1]);
        assert!(
            frame
                .iter()
                .any(|f| (f[0] - on_paper[0]).abs() < 1e-4 && (f[1] - on_paper[1]).abs() < 1e-4),
            "{corner:?} -> {on_paper:?} is no corner of {frame:?}"
        );
    }
}

#[test]
fn a_plot_origin_moves_the_sheet_and_the_limits_are_taken_first() {
    // ANSI B 17 x 11 in unrotated, margins (0.25, 0.75, 0.25, 0.75) in,
    // plot origin (-0.25, -0.5) in: the sheet runs from (0, -0.25) to
    // (17, 10.75), which the file's LIMMIN/LIMMAX say too.
    let tmp = TempDir::new("plot_origin");
    let report = export_fixture("plot_origin_r2000.dxf", &tmp.0, &one_level()).expect("exports");
    assert_eq!(report.sheets.len(), 1);
    let sheet = &report.sheets[0];
    assert_eq!(
        (sheet.name.as_str(), sheet.units.as_str()),
        ("Layout1", "in")
    );
    assert_eq!(sheet.rect_source, "layout_limits");
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(
        close(sheet.rect.min_x, 0.0)
            && close(sheet.rect.min_y, -0.25)
            && close(sheet.rect.max_x, 17.0)
            && close(sheet.rect.max_y, 10.75),
        "{:?}",
        sheet.rect
    );
    assert_eq!(sheet.viewports.len(), 1);
    assert!(sheet.viewports[0].composited);
    assert!((sheet.viewports[0].scale.unwrap() - 0.2).abs() < 1e-9);
}

/// Paper space as a parsed file has it: the entities at the top level
/// beside model space's, and in the `*Paper_Space` block record.
fn add_paper_space(db: &mut CadDatabase, entities: Vec<Entity>) {
    db.entities.extend(entities.iter().cloned());
    db.tables.block_records.insert(
        "*Paper_Space".into(),
        BlockRecord {
            base_point: Default::default(),
            name: "*Paper_Space".into(),
            entities,
        },
    );
}

#[test]
fn a_paper_layout_with_no_usable_rectangle_is_skipped_not_fatal() {
    // A paper space holding nothing but one POINT, and no LAYOUT objects at
    // all -- the R13/R14 / OBJECTS-less DXF shape: the paper block is drawn
    // as a sheet of its own, framed on its entities, and a single point is
    // no rectangle.
    let mut db = model_space(vec![line(0x24, 0.0, 0.0, 100.0, 50.0)]);
    add_paper_space(
        &mut db,
        vec![Entity::Point(PointEntity {
            common: common(0x2A),
            position: p3(100.0, 100.0),
        })],
    );
    let tmp = TempDir::new("paper_point");
    let report = export_db(&db, &tmp.0, &one_level()).expect("a degenerate sheet is not fatal");
    assert!(report.sheets.is_empty(), "{:?}", report.sheets);
    assert_eq!(report.counts.sheets, 0);
    let warning = report
        .warnings
        .iter()
        .find(|w| w.starts_with("UnusableSheet"))
        .unwrap_or_else(|| panic!("{:?}", report.warnings));
    // The fallback names the layout after its block, without the star.
    assert!(
        warning.contains("Paper_Space") && warning.contains("entities"),
        "{warning}"
    );
    assert!(tmp.0.join("manifest.json").exists());
    assert!(!tmp.0.join("sheets").exists());
    let report_json = read_json(&tmp.0.join("report.json"));
    assert!(report_json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().starts_with("UnusableSheet")));
}

#[test]
fn a_paper_block_without_a_layout_is_still_a_sheet() {
    // The same shape with a paper border instead of a point: the sheet is
    // the border's extent, and the model is not composited (there is no
    // viewport to show it through).
    let mut db = model_space(vec![line(0x24, 0.0, 0.0, 100.0, 50.0)]);
    add_paper_space(
        &mut db,
        vec![lwpolyline(
            0x30,
            &[(0.0, 0.0), (420.0, 0.0), (420.0, 297.0), (0.0, 297.0)],
            &[],
            true,
        )],
    );
    let tmp = TempDir::new("paper_border");
    let report = export_db(&db, &tmp.0, &one_level()).expect("exports");
    assert_eq!(report.sheets.len(), 1, "{:?}", report.warnings);
    let sheet = &report.sheets[0];
    assert_eq!(sheet.name, "Paper_Space");
    assert_eq!(sheet.rect_source, "entities");
    assert_eq!(sheet.entities, 1);
    assert!(sheet.viewports.is_empty() && sheet.plot.is_none());
    assert!((sheet.rect.max_x - 420.0).abs() < 1e-9 && (sheet.rect.max_y - 297.0).abs() < 1e-9);
}

#[test]
fn only_on_plan_viewports_composite_and_a_hidden_border_still_does() {
    let tmp = TempDir::new("states");
    let report =
        export_fixture("viewport_states_r2000.dxf", &tmp.0, &one_level()).expect("exports");
    assert_eq!(report.sheets.len(), 2, "{:?}", report.sheets);
    let sheet = &report.sheets[0];
    assert_eq!((sheet.name.as_str(), sheet.tab_order), ("Layout1", 1));
    let states: Vec<(Option<&str>, Option<i32>, bool, bool)> = sheet
        .viewports
        .iter()
        .map(|v| (v.handle.as_deref(), v.viewport_id, v.on, v.composited))
        .collect();
    assert_eq!(
        states,
        [
            // on, plan, layer 0
            (Some("2A"), Some(2), true, true),
            // on, plan, on the frozen layer VPFROZEN: still composited
            (Some("2D"), Some(3), true, true),
            // off
            (Some("2E"), Some(4), false, false),
            // VIEWDIR (1,1,1): not a plan view
            (Some("2F"), Some(5), true, false),
        ]
    );
    for v in &sheet.viewports {
        assert!(!v.overall);
        assert!((v.scale.unwrap() - 2.0).abs() < 1e-9, "{:?}", v.handle);
    }
    // Three of the four borders are drawn: 2D's is on a frozen layer.
    assert_eq!(sheet.entities, 3);
    // The sheet rectangles follow the rotation and the paper unit: A4
    // upside down in mm, and ANSI B turned a quarter in inches -- both from
    // the layouts' limits -- and the second layout's empty paper block is a
    // sheet all the same.
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    for (sheet, unit, want) in [
        (&report.sheets[0], "mm", (-10.0, -20.0, 200.0, 277.0)),
        (&report.sheets[1], "in", (-0.25, -0.25, 10.75, 16.75)),
    ] {
        assert_eq!(sheet.units, unit);
        assert_eq!(sheet.rect_source, "layout_limits");
        assert!(
            close(sheet.rect.min_x, want.0)
                && close(sheet.rect.min_y, want.1)
                && close(sheet.rect.max_x, want.2)
                && close(sheet.rect.max_y, want.3),
            "{}: {:?}",
            sheet.name,
            sheet.rect
        );
    }
    assert_eq!(
        (report.sheets[1].entities, report.sheets[1].viewports.len()),
        (0, 0)
    );
}

#[test]
fn a_sheet_states_the_model_to_paper_mapping_in_the_fields_it_publishes() {
    // The fixture's viewport: a frame 200 x 120 paper units centred at
    // (150, 120), showing a model window of VIEWSIZE 60 centred at (50,
    // 25): scale 2, window (0, -5) .. (100, 55).
    let tmp = TempDir::new("model_to_paper");
    let report = export_fixture("title_block_r2000.dxf", &tmp.0, &one_level()).expect("exports");
    let sheets = read_json(&tmp.0.join("sheets.json"));
    let stated = sheets["model_to_paper"].as_str().expect("the mapping");
    for field in [
        "model_to_paper",
        "model_window",
        "frame",
        "scale",
        "twist_deg",
        "world_to_px",
    ] {
        assert!(stated.contains(field), "{stated}");
    }
    let sheet = &report.sheets[0];
    let vp = sheet
        .viewports
        .iter()
        .find(|v| v.composited)
        .expect("a composited viewport");
    assert!((vp.scale.unwrap() - 2.0).abs() < 1e-9);
    assert_eq!(vp.twist_deg, 0.0);
    let frame = [
        vp.frame.min_x,
        vp.frame.min_y,
        vp.frame.max_x,
        vp.frame.max_y,
    ];
    assert_eq!(frame, [50.0, 60.0, 250.0, 180.0]);
    let window = vp.model_window.expect("a model window");
    assert_eq!(
        window,
        [[0.0, -5.0], [100.0, -5.0], [100.0, 55.0], [0.0, 55.0]]
    );
    // Both statements of the mapping put the window's corners on the
    // frame's, in the order the file names them.
    let m = vp.model_to_paper.expect("the map");
    let by_prose = |p: [f64; 2]| {
        [
            frame[0] + (p[0] - window[0][0]) * 2.0,
            frame[1] + (p[1] - window[0][1]) * 2.0,
        ]
    };
    for (corner, want) in window.iter().zip([
        [frame[0], frame[1]],
        [frame[2], frame[1]],
        [frame[2], frame[3]],
        [frame[0], frame[3]],
    ]) {
        for got in [by_prose(*corner), to_paper(&m, corner[0], corner[1])] {
            assert!(
                (got[0] - want[0]).abs() < 1e-9 && (got[1] - want[1]).abs() < 1e-9,
                "{corner:?} -> {got:?}, not {want:?}"
            );
        }
    }
    // The model LINE (0,0) -> (100,50) is inside the window, so it lands
    // inside the frame.
    for t in [0.0, 0.5, 1.0] {
        let paper = to_paper(&m, 100.0 * t, 50.0 * t);
        assert!(
            (frame[0]..=frame[2]).contains(&paper[0]) && (frame[1]..=frame[3]).contains(&paper[1]),
            "{paper:?} is outside the frame {frame:?}"
        );
    }
}

#[test]
fn the_title_and_the_title_block_are_records_on_their_sheet() {
    // The fixture's model space holds one LINE and no text; its title is
    // on the paper, and so is a title block INSERT holding the sheet
    // number.
    let tmp = TempDir::new("paper_text");
    export_fixture("title_block_r2000.dxf", &tmp.0, &ExportOptions::default()).expect("exports");
    let texts = records(&tmp.0, "texts");
    assert_eq!(texts.len(), 2, "{texts:?}");
    let title = by_handle(&texts, "25");
    assert_eq!(title["id"], "37");
    assert_eq!(title["text"], "GARDEN PAVILION");
    assert_eq!(title["space"], "paper");
    assert_eq!(title["sheet"], "Layout1");
    assert!(title["tiles"].as_array().unwrap().is_empty());
    // The text inside the title block, through the INSERT: id
    // `<insert>/<text>`.
    let sheet_no = by_handle(&texts, "42");
    assert_eq!(sheet_no["id"], format!("{}/{}", 0x26, 0x42));
    assert_eq!(sheet_no["text"], "SHEET 1 OF 2");
    assert_eq!(sheet_no["space"], "paper");
    let strings = read_json(&tmp.0.join("strings.json"));
    assert_eq!(strings["strings"]["garden pavilion"][0], "37");
    assert_eq!(strings["strings"]["sheet 1 of 2"][0], sheet_no["id"]);
    // The pixel box is on the sheet image and nowhere else, where the
    // fixture's own numbers put it: the title anchored at (150, 20) in
    // paper units, 8 units tall.
    let sheets = read_json(&tmp.0.join("sheets.json"));
    let image = &sheets["sheets"][0]["overview"];
    let ppu = image["ppu"].as_f64().unwrap();
    let min_x = image["world"][0].as_f64().unwrap();
    let max_y = image["world"][3].as_f64().unwrap();
    let px = title["px"]["sheet:Layout1"].as_array().unwrap();
    let (x0, y0, x1, y1) = (
        px[0].as_f64().unwrap(),
        px[1].as_f64().unwrap(),
        px[2].as_f64().unwrap(),
        px[3].as_f64().unwrap(),
    );
    assert!((x0 - (150.0 - min_x) * ppu).abs() <= 4.0, "{x0}");
    assert!((y1 - (max_y - 20.0) * ppu).abs() <= 4.0, "{y1}");
    let (w, h) = (x1 - x0, y1 - y0);
    assert!(h > 0.4 * 8.0 * ppu && h < 1.5 * 8.0 * ppu, "{h}");
    assert!(w > h, "{w}x{h}");
    assert_eq!(title["px"].as_object().unwrap().len(), 1);
    let manifest = read_json(&tmp.0.join("manifest.json"));
    assert_eq!(manifest["counts"]["texts"], 2);
    assert_eq!(manifest["counts"]["texts_paper"], 2);
    assert_eq!(manifest["capabilities"]["paper_text"], "indexed");
    assert_eq!(manifest["sheets"][0]["texts"], 2);
}
