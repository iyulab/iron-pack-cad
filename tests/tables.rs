//! A table (ACAD_TABLE) in the package: a block instance whose record says
//! what its cells say, by row and column, and whose cell texts lead to it
//! from the string index.

mod common;

use common::*;
use iron_pack_cad::ExportOptions;
use serde_json::{json, Value};
use uncad_model::model::{
    AcadTableEntity, Entity, Point3D, Ref, TableCell, TableCellKind, TableGrid, TableRow,
};
use uncad_model::CadDatabase;

fn cell(text: Option<&str>, covered: bool, span_columns: u32) -> TableCell {
    TableCell {
        kind: Some(TableCellKind::Text),
        text: text.map(str::to_string),
        covered,
        span_columns,
        span_rows: 1,
    }
}

/// A parts list at (10, 20): a title over both columns, then a material
/// written with an MTEXT font switch and a quantity.
fn parts_list(grid: Option<TableGrid>) -> CadDatabase {
    let table = Entity::AcadTable(AcadTableEntity {
        common: common(7),
        block_name: Ref::Resolved("*T1".into()),
        insertion_point: p3(10.0, 20.0),
        scale: Point3D {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        rotation: 0.0,
        grid,
    });
    // What the table's block draws: its frame and its texts, from the
    // origin.
    let block = vec![
        line(900, 0.0, 0.0, 30.0, 0.0),
        line(901, 0.0, -9.0, 30.0, -9.0),
        mtext(902, 1.0, -1.0, 2.5, "PARTS"),
        mtext(903, 1.0, -6.0, 2.5, r"{\fArial;S45C}"),
        mtext(904, 21.0, -6.0, 2.5, "4"),
    ];
    with_blocks(vec![table], vec![("*T1", block)])
}

fn grid() -> TableGrid {
    TableGrid {
        column_widths: vec![20.0, 10.0],
        rows: vec![
            TableRow {
                height: 5.0,
                cells: vec![cell(Some("PARTS"), false, 2), cell(None, true, 1)],
            },
            TableRow {
                height: 4.0,
                cells: vec![
                    cell(Some(r"{\fArial;S45C}"), false, 1),
                    cell(Some("4"), false, 1),
                ],
            },
        ],
    }
}

fn table_record(dir: &std::path::Path) -> Value {
    records(dir, "blocks")
        .into_iter()
        .find(|r| r["id"] == "7")
        .expect("the table's block instance record")
}

#[test]
fn a_table_is_a_block_instance_with_its_cells() {
    let tmp = TempDir::new("table-cells");
    export_db(&parts_list(Some(grid())), &tmp.0, &ExportOptions::default()).expect("exports");
    let r = table_record(&tmp.0);
    assert_eq!(r["block"], "*T1");
    assert_eq!(r["at"], json!([10.0, 20.0]));
    assert!(r.get("attribs").is_none(), "{r}");
    let t = &r["table"];
    assert_eq!(
        (t["rows"].clone(), t["columns"].clone()),
        (json!(2), json!(2))
    );
    assert_eq!(
        t["cells"],
        json!([
            {"row": 0, "column": 0, "text": "PARTS", "span": [2, 1]},
            {"row": 1, "column": 0, "text": "S45C", "raw": r"{\fArial;S45C}", "span": [1, 1]},
            {"row": 1, "column": 1, "text": "4", "span": [1, 1]},
        ])
    );
}

#[test]
fn a_cells_text_leads_to_the_table() {
    let tmp = TempDir::new("table-strings");
    export_db(&parts_list(Some(grid())), &tmp.0, &ExportOptions::default()).expect("exports");
    let strings = read_json(&tmp.0.join("strings.json"));
    let ids = strings["strings"]["s45c"]
        .as_array()
        .unwrap_or_else(|| panic!("s45c is indexed: {strings}"));
    // The table, and the text its block draws, under the table's ID.
    assert!(ids.contains(&json!("7")), "{ids:?}");
    assert!(ids.contains(&json!("7/903")), "{ids:?}");
}

#[test]
fn a_table_whose_cells_were_not_read_says_so() {
    let tmp = TempDir::new("table-unread");
    export_db(&parts_list(None), &tmp.0, &ExportOptions::default()).expect("exports");
    let t = &table_record(&tmp.0)["table"];
    assert!(
        t["rows"].is_null() && t["columns"].is_null() && t["cells"].is_null(),
        "{t}"
    );
    assert!(t["why"].as_str().is_some_and(|w| !w.is_empty()), "{t}");
}

#[test]
fn the_tables_block_lists_the_table_among_its_instances() {
    let tmp = TempDir::new("table-instances");
    export_db(&parts_list(Some(grid())), &tmp.0, &ExportOptions::default()).expect("exports");
    let drawing = read_json(&tmp.0.join("drawing.json"));
    let block = drawing["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == "*T1")
        .unwrap_or_else(|| panic!("*T1 among the definitions: {}", drawing["blocks"]));
    assert_eq!(block["instance_ids"], json!(["7"]), "{block}");
}
