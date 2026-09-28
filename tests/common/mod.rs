//! What the package tests share: the DXF fixtures read into the model with
//! the header variables they state, a scratch directory, JSON reading, record
//! collection across shards, and small drawings built on the model's own
//! types -- model space is what the renderer draws, so an entity has to be
//! both at the top level and in the `*Model_Space` block record.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use iron_pack_cad::{export_package, ExportError, ExportOptions, ExportReport, Header};
use serde_json::{Map, Value};
use uncad_model::model::{
    AttribEntity, Confidence, Entity, EntityCommon, EntityId, EntityLinetype, InsertEntity,
    LineEntity, LwPolylineEntity, MTextAttachment, MTextEntity, Origin, Point2D, Point3D,
    PolylineVertex, Ref, TextEntity,
};
use uncad_model::tables::BlockRecord;
use uncad_model::{CadDatabase, Tables};

/// The bytes of one of `tests/fixtures`.
fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// One of `tests/fixtures`, read into the model, with the header variables
/// it states.
pub fn fixture(name: &str) -> (CadDatabase, Header) {
    let bytes = fixture_bytes(name);
    let (db, header) =
        undxf::read_bytes_with_header(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
    (db, header_of(&header))
}

/// The variables a DXF's HEADER section states, as a [`Header`]: each
/// variable under its name in lower case -- a point for one written as
/// 10/20(/30) groups, a number when its value reads as one, the text
/// otherwise. `$ACADVER` is the header's `acadver`; `$DWGCODEPAGE` its
/// `codepage_name`.
fn header_of(header: &undxf::Header) -> Header {
    let mut vars = Map::new();
    vars.insert("format".into(), Value::from("dxf"));
    for name in header.variables.keys() {
        let key = match name.as_str() {
            "DWGCODEPAGE" => "codepage_name".to_string(),
            other => other.to_lowercase(),
        };
        let value = if let Some(p) = header.point3(name) {
            serde_json::json!({ "x": p.x, "y": p.y, "z": p.z })
        } else if let Some(p) = header.point2(name) {
            serde_json::json!({ "x": p.x, "y": p.y })
        } else if let Some(i) = header.int(name) {
            Value::from(i)
        } else if let Some(r) = header.real(name) {
            Value::from(r)
        } else {
            Value::from(header.text(name).unwrap_or_default())
        };
        vars.insert(key, value);
    }
    serde_json::from_value(Value::Object(vars)).expect("the header's variables")
}

/// A fresh directory under the target dir, removed when dropped.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(name: &str) -> TempDir {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("export_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every record of `name` (`texts`, `geometry`, ...), whether written whole
/// or in shards.
pub fn records(dir: &Path, name: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let single = dir.join(format!("{name}.json"));
    if single.exists() {
        out.extend(read_json(&single)["records"].as_array().unwrap().clone());
        return out;
    }
    let mut shards: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&format!("{name}.")) && n.ends_with(".json"))
        })
        .collect();
    shards.sort();
    assert!(!shards.is_empty(), "{name}.json or its shards");
    for shard in shards {
        out.extend(read_json(&shard)["records"].as_array().unwrap().clone());
    }
    out
}

/// The record of `records` whose `handle` is `handle`.
pub fn by_handle<'a>(records: &'a [Value], handle: &str) -> &'a Value {
    records
        .iter()
        .find(|r| r["handle"] == handle)
        .unwrap_or_else(|| panic!("a record with handle {handle}"))
}

/// One of `tests/golden` -- a golden case as the entity model's golden
/// writer wrote it -- read into the model, with its header.
pub fn golden(name: &str) -> (CadDatabase, Header) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let (db, header) =
        undxf::read_bytes_with_header(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
    (db, header_of(&header))
}

/// Reads a golden case with its header and exports it.
pub fn export_golden(
    name: &str,
    dir: &Path,
    options: &ExportOptions,
) -> Result<ExportReport, ExportError> {
    let (db, header) = golden(name);
    export_package(&db, Some(&header), dir, options)
}

/// Reads a fixture with its header and exports it.
pub fn export_fixture(
    name: &str,
    dir: &Path,
    options: &ExportOptions,
) -> Result<ExportReport, ExportError> {
    let (db, header) = fixture(name);
    export_package(&db, Some(&header), dir, options)
}

/// Exports a drawing built here, which has no header.
pub fn export_db(
    db: &CadDatabase,
    dir: &Path,
    options: &ExportOptions,
) -> Result<ExportReport, ExportError> {
    export_package(db, None, dir, options)
}

/// The width and height a PNG's header states (IHDR, bytes 16..24, big
/// endian). The file's size, not its content.
pub fn png_size(png: &[u8]) -> [u32; 2] {
    assert_eq!(&png[12..16], b"IHDR", "a PNG starts with its IHDR chunk");
    let be = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
    [be(16), be(20)]
}

// ------------------------------------------------------------ drawings

/// An entity's common fields: its ID `id`, its handle the ID in hex, on
/// layer 0, by layer, as a reader of a file would state them.
pub fn common(id: u64) -> EntityCommon {
    EntityCommon {
        id: EntityId::new(id),
        origin: Origin::Vector,
        confidence: Confidence::High,
        source_handle: Ref::Resolved(format!("{id:X}")),
        layer: Ref::Resolved("0".into()),
        color_index: 256,
        true_color: None,
        invisible: false,
        linetype: EntityLinetype::ByLayer,
        linetype_scale: 1.0,
        lineweight: None,
        transparency: None,
    }
}

pub fn on_layer(mut common: EntityCommon, layer: &str) -> EntityCommon {
    common.layer = Ref::Resolved(layer.into());
    common
}

pub fn p3(x: f64, y: f64) -> Point3D {
    Point3D { x, y, z: 0.0 }
}

pub fn z_axis() -> Point3D {
    Point3D {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    }
}

pub fn line(id: u64, x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    Entity::Line(LineEntity {
        common: common(id),
        start_point: p3(x0, y0),
        end_point: p3(x1, y1),
    })
}

pub fn text(id: u64, x: f64, y: f64, height: f64, s: &str) -> Entity {
    Entity::Text(TextEntity {
        common: common(id),
        start_point: Point2D { x, y },
        text_height: height,
        text: s.into(),
        rotation: 0.0,
        horizontal_justification: Default::default(),
        vertical_justification: Default::default(),
        alignment_point: None,
        width_factor: 1.0,
        oblique_angle: 0.0,
        style_name: Ref::Absent,
        elevation: 0.0,
        extrusion: z_axis(),
    })
}

pub fn mtext(id: u64, x: f64, y: f64, height: f64, s: &str) -> Entity {
    Entity::MText(MTextEntity {
        common: common(id),
        insertion_point: p3(x, y),
        text: s.into(),
        text_height: height,
        rotation: 0.0,
        line_spacing_factor: 1.0,
        attachment: Some(MTextAttachment::TopLeft),
        reference_width: 0.0,
        extents_width: None,
        extents_height: None,
        style_name: Ref::Absent,
    })
}

pub fn lwpolyline(id: u64, points: &[(f64, f64)], bulges: &[f64], closed: bool) -> Entity {
    Entity::LwPolyline(LwPolylineEntity {
        common: common(id),
        vertices: points
            .iter()
            .enumerate()
            .map(|(i, (x, y))| PolylineVertex {
                point: Point2D { x: *x, y: *y },
                bulge: bulges.get(i).copied().unwrap_or(0.0),
                start_width: 0.0,
                end_width: 0.0,
            })
            .collect(),
        closed,
        const_width: 0.0,
        elevation: 0.0,
        extrusion: z_axis(),
    })
}

/// A drawing whose model space is `entities`, wired up the way a parsed
/// file has it.
pub fn model_space(entities: Vec<Entity>) -> CadDatabase {
    with_blocks(entities, Vec::new())
}

/// [`model_space`] with block definitions beside it.
pub fn with_blocks(entities: Vec<Entity>, blocks: Vec<(&str, Vec<Entity>)>) -> CadDatabase {
    let mut tables = Tables::default();
    tables.block_records.insert(
        "*Model_Space".into(),
        BlockRecord {
            base_point: Default::default(),
            name: "*Model_Space".into(),
            entities: entities.clone(),
        },
    );
    for (name, block) in blocks {
        tables.block_records.insert(
            name.into(),
            BlockRecord {
                base_point: Default::default(),
                name: name.into(),
                entities: block,
            },
        );
    }
    CadDatabase {
        entities,
        tables,
        read_diagnostics: Default::default(),
    }
}

/// An INSERT of `block` at (`x`, `y`), unscaled and unrotated, carrying
/// `attribs`.
pub fn insert(id: u64, block: &str, x: f64, y: f64, attribs: Vec<AttribEntity>) -> Entity {
    Entity::Insert(InsertEntity {
        common: common(id),
        block_name: Ref::Resolved(block.into()),
        insertion_point: p3(x, y),
        scale: Point3D {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        rotation: 0.0,
        attribs,
        extrusion: z_axis(),
    })
}

/// An ATTRIB `tag` = `value` at (`x`, `y`), height 2.5.
pub fn attrib(id: u64, x: f64, y: f64, tag: &str, value: &str) -> AttribEntity {
    AttribEntity {
        common: common(id),
        start_point: Point2D { x, y },
        text_height: 2.5,
        tag: tag.into(),
        flags: Default::default(),
        text: value.into(),
        rotation: 0.0,
        horizontal_justification: Default::default(),
        vertical_justification: Default::default(),
        alignment_point: None,
        width_factor: 1.0,
        oblique_angle: 0.0,
        style_name: Ref::Absent,
        elevation: 0.0,
        extrusion: z_axis(),
    }
}

/// A drawing with a little of everything the records carry, spread over
/// 610 x 350 units: a grid of 200 lines on the layers `GRID-A` and
/// `GRID-B`, six texts and an MTEXT, three closed outlines on `AREA`, and
/// twelve INSERTs of a four-line block `BOX`, each with an ATTRIB `NO` =
/// `BOX-01` .. `BOX-12`.
pub fn sample_drawing() -> CadDatabase {
    let mut entities = Vec::new();
    for i in 0..200u64 {
        let (x, y) = ((i % 20) as f64 * 20.0, (i / 20) as f64 * 30.0);
        let mut grid = line(0x1000 + i, x, y, x + 15.0, y + 5.0);
        let layer = if i % 2 == 0 { "GRID-A" } else { "GRID-B" };
        *grid.common_mut() = on_layer(grid.common().clone(), layer);
        entities.push(grid);
    }
    for (i, s) in [
        "PLAN",
        "SECTION A-A",
        "NOTE 1",
        "NOTE 2",
        "SCALE 1:10",
        "REV B",
    ]
    .iter()
    .enumerate()
    {
        entities.push(text(
            0x100 + i as u64,
            10.0 + 60.0 * i as f64,
            310.0,
            5.0,
            s,
        ));
    }
    entities.push(mtext(
        0x110,
        10.0,
        340.0,
        3.5,
        r"GENERAL NOTES\PALL DIMENSIONS IN MM",
    ));
    for (i, (x, y)) in [(420.0, 0.0), (420.0, 100.0), (420.0, 200.0)]
        .iter()
        .enumerate()
    {
        let mut outline = lwpolyline(
            0x120 + i as u64,
            &[
                (*x, *y),
                (x + 50.0, *y),
                (x + 50.0, y + 40.0),
                (*x, y + 40.0),
            ],
            &[],
            true,
        );
        *outline.common_mut() = on_layer(outline.common().clone(), "AREA");
        entities.push(outline);
    }
    for i in 0..12u64 {
        let (x, y) = (500.0 + (i % 4) as f64 * 30.0, (i / 4) as f64 * 30.0);
        let value = attrib(
            0x300 + i,
            x + 2.0,
            y + 2.0,
            "NO",
            &format!("BOX-{:02}", i + 1),
        );
        entities.push(insert(0x200 + i, "BOX", x, y, vec![value.clone()]));
        // A top-level INSERT's attribute values are top-level entities of
        // the model as well, as a reader lists them.
        entities.push(Entity::Attrib(value));
    }
    with_blocks(
        entities,
        vec![(
            "BOX",
            vec![
                line(0x50, 0.0, 0.0, 20.0, 0.0),
                line(0x51, 20.0, 0.0, 20.0, 20.0),
                line(0x52, 20.0, 20.0, 0.0, 20.0),
                line(0x53, 0.0, 20.0, 0.0, 0.0),
            ],
        )],
    )
}

/// The header a reader would give [`sample_drawing`]'s file: a DXF in
/// millimetres.
pub fn sample_header() -> Header {
    Header {
        format: Some("dxf".into()),
        acadver: Some("AC1015".into()),
        insunits: Some(4),
        ..Default::default()
    }
}
