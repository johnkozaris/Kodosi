use std::io::Write as _;

use document::{Assets, Format, Ordered, Refused};
use package::Package;
use serde_json::json;

use crate::{Change, Excel, Model, Op, Query, Value, model::CellChange};

const SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:B4"/><sheetData><row r="1" spans="1:2" x14ac:custom="kept"><c r="A1" s="1"><v>2</v></c><c r="B1" t="s"><v>0</v></c></row><row r="2"><c r="A2"><v>3</v></c></row><row r="3"><c r="A3"><f>A1+A2</f><v>5</v></c><c r="B3"><f t="shared" ref="B3:B4" si="0">A3*2</f><v>10</v></c></row><row r="4"><c r="B4"><f t="shared" si="0"/><v>0</v></c></row></sheetData><mergeCells count="1"><mergeCell ref="A9:B9"/></mergeCells></worksheet>"#;
const WORKBOOK: &str = r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
const RELATIONS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/></Relationships>"#;
const STRINGS: &str = r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>hello</t></si></sst>"#;
const STYLES: &str = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="12"/><color rgb="FFFF0000"/><name val="Arial"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border/></borders><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0"/></cellXfs></styleSheet>"#;
const TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>"#;
const ROOT_RELATIONS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

fn excel_file() -> Vec<u8> {
    excel_file_with(
        &SHEET.replace(" x14ac:custom=\"kept\"", " ht=\"20\""),
        WORKBOOK,
        STYLES,
    )
}

fn excel_file_with(sheet: &str, workbook: &str, styles: &str) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in [
        ("[Content_Types].xml", TYPES),
        ("_rels/.rels", ROOT_RELATIONS),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", RELATIONS),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/sharedStrings.xml", STRINGS),
        ("xl/styles.xml", styles),
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn read(file: &[u8]) -> Model {
    Excel::read(file, &mut Assets::default()).unwrap()
}

fn part(file: &[u8], name: &str) -> String {
    Package::open(file).unwrap().text(name).unwrap()
}

fn ops(model: &mut Model, ops: Vec<Op>) -> Result<Change, Refused> {
    Excel::apply(model, Change::Ops { ops }, &[], &mut Assets::default())
}

fn saved(model: &Model, file: &[u8]) -> Vec<u8> {
    Excel::write(model, file, &Assets::default()).unwrap()
}

fn set(cell: &str, value: Value) -> Op {
    Op::SetCell {
        sheet: "Data".to_owned(),
        cell: cell.to_owned(),
        value: Some(value),
        formula: None,
    }
}

fn cells(model: &Model) -> serde_json::Value {
    Excel::query(
        model,
        Query::Read {
            sheet: None,
            range: None,
        },
    )
    .0["sheets"][0]["cells"]
        .clone()
}

fn cell_value(model: &Model, name: &str) -> serde_json::Value {
    cells(model)
        .as_array()
        .unwrap()
        .iter()
        .find(|cell| cell["cell"] == name)
        .map_or(serde_json::Value::Null, |cell| cell["value"].clone())
}

#[test]
fn no_edit_means_the_same_bytes() {
    let file = excel_file();
    let model = read(&file);
    assert_eq!(saved(&model, &file), file);
}

#[test]
fn cells_come_with_values_formulas_and_looks() {
    let model = read(&excel_file());
    let snapshot = Excel::snapshot(&model).0;
    assert_eq!(snapshot["sheets"][0]["name"], "Data");
    assert_eq!(snapshot["sheets"][0]["cells"]["0"]["1"]["value"], "hello");
    assert_eq!(snapshot["sheets"][0]["cells"]["2"]["0"]["formula"], "A1+A2");
    assert_eq!(snapshot["sheets"][0]["cells"]["3"]["1"]["formula"], "A4*2");
    assert_eq!(snapshot["sheets"][0]["merges"], json!([[8, 0, 8, 1]]));
    assert_eq!(snapshot["looks"][1]["bold"], true);
    assert_eq!(snapshot["looks"][1]["color"], "FF0000");
}

#[test]
fn an_edit_changes_one_cell_and_the_results_that_follow_it() {
    let file = excel_file();
    let mut model = read(&file);
    ops(&mut model, vec![set("A2", Value::Number(10.0))]).unwrap();
    let new_file = saved(&model, &file);
    let xml = part(&new_file, "xl/worksheets/sheet1.xml");
    assert!(xml.contains(r#"<c r="A2"><v>10</v></c>"#));
    assert!(xml.contains(r#"<row r="1" spans="1:2" ht="20">"#));
    assert_eq!(
        part(&new_file, "xl/sharedStrings.xml"),
        part(&file, "xl/sharedStrings.xml")
    );
    let again = read(&new_file);
    assert_eq!(cell_value(&again, "A3"), 12.0);
    assert_eq!(cell_value(&again, "B3"), 24.0);
}

#[test]
fn a_read_sees_results_before_any_save() {
    let file = excel_file();
    let mut model = read(&file);
    ops(&mut model, vec![set("A2", Value::Number(10.0))]).unwrap();
    assert_eq!(cell_value(&model, "A3"), 12.0);
}

#[test]
fn a_formula_uses_the_names_of_the_workbook_and_keeps_what_it_cannot_work_out() {
    let sheet = SHEET.replace(" x14ac:custom=\"kept\"", "").replace(
        r#"<c r="B1" t="s"><v>0</v></c>"#,
        r#"<c r="B1" t="s"><v>0</v></c><c r="C1"><f>SUM(Top)</f><v>0</v></c><c r="D1"><f>KODOSIUNKNOWN(1)</f><v>7</v></c>"#,
    );
    let workbook = WORKBOOK.replace(
        "</sheets>",
        r#"</sheets><definedNames><definedName name="Top">Data!$A$2:$A$3</definedName></definedNames>"#,
    );
    let mut model = read(&excel_file_with(&sheet, &workbook, STYLES));
    ops(&mut model, vec![set("A2", Value::Number(10.0))]).unwrap();
    assert_eq!(cell_value(&model, "C1"), 22.0);
    assert_eq!(cell_value(&model, "D1"), 7.0);
}

#[test]
fn new_text_keeps_the_shared_strings_as_they_are() {
    let file = excel_file();
    let mut model = read(&file);
    ops(&mut model, vec![set("C1", Value::Text("a <b>".to_owned()))]).unwrap();
    let new_file = saved(&model, &file);
    let xml = part(&new_file, "xl/worksheets/sheet1.xml");
    assert!(xml.contains(
        r#"<c r="C1" t="inlineStr"><is><t xml:space="preserve">a &lt;b&gt;</t></is></c>"#
    ));
    assert_eq!(cell_value(&read(&new_file), "C1"), "a <b>");
}

#[test]
fn a_new_row_goes_in_its_place() {
    let file = excel_file();
    let mut model = read(&file);
    ops(
        &mut model,
        vec![set("A20", Value::Number(1.0)), set("B2", Value::Bool(true))],
    )
    .unwrap();
    let again = read(&saved(&model, &file));
    assert_eq!(cell_value(&again, "A20"), 1.0);
    assert_eq!(cell_value(&again, "B2"), true);
}

#[test]
fn clearing_a_cell_removes_it() {
    let file = excel_file();
    let mut model = read(&file);
    ops(
        &mut model,
        vec![Op::ClearRange {
            sheet: "Data".to_owned(),
            range: "B1:B1".to_owned(),
        }],
    )
    .unwrap();
    let again = read(&saved(&model, &file));
    assert_eq!(cell_value(&again, "B1"), serde_json::Value::Null);
}

#[test]
fn a_new_look_adds_to_the_style_table_and_keeps_the_old_ones() {
    let file = excel_file();
    let mut model = read(&file);
    ops(
        &mut model,
        vec![Op::FormatRange {
            sheet: "Data".to_owned(),
            range: "A2:A2".to_owned(),
            look: crate::model::Look {
                bold: true,
                strike: true,
                fill: Some("00FF00".to_owned()),
                number_format: Some("0.00%".to_owned()),
                vertical: Some("center".to_owned()),
                ..crate::model::Look::default()
            },
        }],
    )
    .unwrap();
    let new_file = saved(&model, &file);
    let again = read(&new_file);
    let snapshot = Excel::snapshot(&again).0;
    let style = snapshot["sheets"][0]["cells"]["1"]["0"]["style"]
        .as_u64()
        .unwrap();
    assert_eq!(style, 2);
    assert_eq!(snapshot["looks"][2]["fill"], "00FF00");
    assert_eq!(snapshot["looks"][2]["numberFormat"], "0.00%");
    assert_eq!(snapshot["looks"][2]["strike"], true);
    assert_eq!(snapshot["looks"][2]["vertical"], "center");
    assert_eq!(snapshot["looks"][1]["color"], "FF0000");
}

#[test]
fn a_look_goes_out_with_its_style_so_other_views_can_draw_it() {
    let mut model = read(&excel_file());
    let look = crate::model::Look {
        italic: true,
        ..crate::model::Look::default()
    };
    let change = Change::Cells {
        sheet: model.sheets[0].id.clone(),
        cells: vec![CellChange {
            row: 0,
            column: 0,
            value: None,
            formula: None,
            style: None,
            look: Some(look.clone()),
        }],
    };
    let sent = Excel::apply(&mut model, change, &[], &mut Assets::default()).unwrap();
    let Change::Cells { cells, .. } = sent else {
        panic!("a cells change stays a cells change");
    };
    assert_eq!(cells[0].look, Some(look));
    assert_eq!(cells[0].style, Some(Some(2)));
}

fn format(model: &mut Model, range: &str, look: crate::model::Look) {
    ops(
        model,
        vec![Op::FormatRange {
            sheet: "Data".to_owned(),
            range: range.to_owned(),
            look,
        }],
    )
    .unwrap();
}

#[test]
fn each_save_adds_only_the_looks_that_the_file_lacks() {
    let file = excel_file();
    let mut model = read(&file);
    let bold = crate::model::Look {
        bold: true,
        ..crate::model::Look::default()
    };
    format(&mut model, "A2:A2", bold.clone());
    let first = saved(&model, &file);
    let filled = crate::model::Look {
        fill: Some("00FF00".to_owned()),
        ..bold
    };
    format(&mut model, "A2:A2", filled);
    let second = saved(&model, &first);
    let snapshot = Excel::snapshot(&read(&second)).0;
    let style = snapshot["sheets"][0]["cells"]["1"]["0"]["style"]
        .as_u64()
        .unwrap();
    assert_eq!(
        snapshot["looks"][usize::try_from(style).unwrap()]["fill"],
        "00FF00"
    );
    assert_eq!(part(&second, "xl/styles.xml").matches("<xf ").count(), 4);
}

#[test]
fn number_formats_keep_their_ids_and_take_free_ones() {
    let styles = STYLES
        .replace(
            "<fonts",
            r#"<numFmts count="1"><numFmt numFmtId="164" formatCode="0.0"/></numFmts><fonts"#,
        )
        .replace(
            r#"<xf numFmtId="0" fontId="1""#,
            r#"<xf numFmtId="14" fontId="1""#,
        );
    let file = excel_file_with(
        &SHEET.replace(" x14ac:custom=\"kept\"", ""),
        WORKBOOK,
        &styles,
    );
    let mut model = read(&file);
    assert_eq!(
        Excel::snapshot(&model).0["looks"][1]["numberFormat"],
        "mm-dd-yy"
    );
    for code in ["0.0", "0.000", "0.00%"] {
        format(
            &mut model,
            "A2:A2",
            crate::model::Look {
                number_format: Some(code.to_owned()),
                ..crate::model::Look::default()
            },
        );
    }
    let styles = part(&saved(&model, &file), "xl/styles.xml");
    assert!(styles.contains(r#"<xf numFmtId="164""#));
    assert!(styles.contains(r#"<numFmt numFmtId="165" formatCode="0.000"/>"#));
    assert!(styles.contains(r#"<xf numFmtId="10""#));
}

#[test]
fn inserted_rows_move_cells_and_formulas() {
    let file = excel_file();
    let mut model = read(&file);
    ops(
        &mut model,
        vec![Op::InsertRows {
            sheet: "Data".to_owned(),
            at: 1,
            count: 2,
        }],
    )
    .unwrap();
    let new_file = saved(&model, &file);
    let again = read(&new_file);
    let snapshot = Excel::snapshot(&again).0;
    assert_eq!(snapshot["sheets"][0]["cells"]["4"]["0"]["formula"], "A1+A4");
    assert_eq!(cell_value(&again, "A5"), 5.0);
    assert_eq!(cell_value(&again, "A4"), 3.0);
}

#[test]
fn sizes_and_merges_reach_the_file() {
    let file = excel_file();
    let mut model = read(&file);
    let mut apply = |change| Excel::apply(&mut model, change, &[], &mut Assets::default());
    apply(Change::Size {
        sheet: "1".to_owned(),
        widths: [(1, 30.0)].into(),
        heights: [(1, 44.0), (9, 12.0)].into(),
    })
    .unwrap();
    apply(Change::Merges {
        sheet: "1".to_owned(),
        add: vec![[0, 2, 1, 3]],
        remove: vec![[8, 0, 8, 1]],
    })
    .unwrap();
    let new_file = saved(&model, &file);
    let xml = part(&new_file, "xl/worksheets/sheet1.xml");
    assert!(xml.contains(r#"<col min="2" max="2" width="30" customWidth="1"/>"#));
    assert!(xml.contains(r#"<row r="2" ht="44" customHeight="1">"#));
    assert!(xml.contains(r#"<row r="10" ht="12" customHeight="1">"#));
    assert!(xml.contains(r#"<mergeCell ref="C1:D2"/>"#));
    assert!(!xml.contains("A9:B9"));
    let again = read(&new_file);
    assert_eq!(saved(&again, &new_file), new_file);
    let snapshot = Excel::snapshot(&again).0;
    assert_eq!(snapshot["sheets"][0]["widths"]["1"], 30.0);
    assert_eq!(snapshot["sheets"][0]["heights"]["1"], 44.0);
    assert_eq!(snapshot["sheets"][0]["merges"], json!([[0, 2, 1, 3]]));
}

#[test]
fn moved_rows_take_what_points_at_them() {
    let sheet = SHEET
        .replace(" x14ac:custom=\"kept\"", "")
        .replace(
            r#"<row r="2">"#,
            r#"<row r="2" ht="33" s="1" customFormat="1">"#,
        )
        .replace(
            "</mergeCells>",
            r#"</mergeCells><conditionalFormatting sqref="A2:A3 B4"><cfRule type="cellIs" priority="1" operator="greaterThan"><formula>1</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="A1"><cfRule type="cellIs" priority="2" operator="greaterThan"><formula>1</formula></cfRule></conditionalFormatting>"#,
        );
    let workbook = WORKBOOK.replace(
        "</sheets>",
        r#"</sheets><definedNames><definedName name="Top">Data!$A$2:$A$3</definedName></definedNames>"#,
    );
    let file = excel_file_with(&sheet, &workbook, STYLES);
    let mut model = read(&file);
    ops(
        &mut model,
        vec![Op::InsertRows {
            sheet: "Data".to_owned(),
            at: 1,
            count: 2,
        }],
    )
    .unwrap();
    let new_file = saved(&model, &file);
    let xml = part(&new_file, "xl/worksheets/sheet1.xml");
    assert!(xml.contains(r#"<row r="4" s="1" customFormat="1" ht="33" customHeight="1">"#));
    assert!(xml.contains(r#"sqref="A4:A5 B6""#));
    assert!(xml.contains(r#"sqref="A1""#));
    assert!(xml.contains(r#"ref="A11:B11""#));
    assert!(part(&new_file, "xl/workbook.xml").contains("Data!$A$4:$A$5"));
    let again = read(&new_file);
    assert_eq!(Excel::snapshot(&again).0["sheets"][0]["heights"]["3"], 33.0);
}

#[test]
fn a_sheet_from_a_view_keeps_its_cells_across_saves() {
    let file = excel_file();
    let mut model = read(&file);
    let cell = |row, value: f64| Change::Cells {
        sheet: "from-view".to_owned(),
        cells: vec![CellChange {
            row,
            column: 0,
            value: Some(Some(Value::Number(value))),
            ..CellChange::default()
        }],
    };
    let apply = |model: &mut Model, change| {
        Excel::apply(model, change, &[], &mut Assets::default()).unwrap();
    };
    apply(
        &mut model,
        Change::AddSheet {
            id: "from-view".to_owned(),
            name: "Plan".to_owned(),
        },
    );
    apply(&mut model, cell(0, 1.0));
    let first = saved(&model, &file);
    apply(&mut model, cell(1, 2.0));
    let second = saved(&model, &first);
    let snapshot = Excel::snapshot(&read(&second)).0;
    assert_eq!(snapshot["sheets"].as_array().unwrap().len(), 2);
    assert_eq!(snapshot["sheets"][1]["name"], "Plan");
    assert_eq!(snapshot["sheets"][1]["cells"]["0"]["0"]["value"], 1.0);
    assert_eq!(snapshot["sheets"][1]["cells"]["1"]["0"]["value"], 2.0);
}

#[test]
fn a_removed_sheet_leaves_no_part_behind() {
    let file = excel_file();
    let mut model = read(&file);
    let apply = |model: &mut Model, change| {
        Excel::apply(model, change, &[], &mut Assets::default()).unwrap();
    };
    apply(
        &mut model,
        Change::AddSheet {
            id: "added".to_owned(),
            name: "Added".to_owned(),
        },
    );
    let with_sheet = saved(&model, &file);
    assert!(
        Package::open(&with_sheet)
            .unwrap()
            .has("xl/worksheets/sheetN2.xml")
    );
    apply(
        &mut model,
        Change::RemoveSheet {
            sheet: "added".to_owned(),
        },
    );
    let without_sheet = saved(&model, &with_sheet);
    assert!(
        !Package::open(&without_sheet)
            .unwrap()
            .has("xl/worksheets/sheetN2.xml")
    );
    assert!(!part(&without_sheet, "xl/_rels/workbook.xml.rels").contains("sheetN2"));
    assert!(!part(&without_sheet, "[Content_Types].xml").contains("sheetN2"));
    assert!(part(&without_sheet, "[Content_Types].xml").contains("sheet1.xml"));
}

#[test]
fn a_new_sheet_never_takes_the_number_of_a_removed_one() {
    let file = excel_file();
    let mut model = read(&file);
    let mut apply = |change| {
        Excel::apply(&mut model, change, &[], &mut Assets::default()).unwrap();
    };
    apply(Change::AddSheet {
        id: "first".to_owned(),
        name: "First".to_owned(),
    });
    apply(Change::RemoveSheet {
        sheet: "1".to_owned(),
    });
    apply(Change::AddSheet {
        id: "second".to_owned(),
        name: "Second".to_owned(),
    });
    let snapshot = Excel::snapshot(&read(&saved(&model, &file))).0;
    let names = snapshot["sheets"].as_array().unwrap();
    assert_eq!(names.len(), 2);
    assert_eq!(names[0]["name"], "First");
    assert_eq!(names[1]["name"], "Second");
    assert_eq!(names[0]["cells"], json!({}));
}

#[test]
fn a_new_sheet_goes_in_the_package() {
    let file = excel_file();
    let mut model = read(&file);
    ops(
        &mut model,
        vec![Op::AddSheet {
            name: "Totals".to_owned(),
        }],
    )
    .unwrap();
    ops(
        &mut model,
        vec![Op::SetCell {
            sheet: "Totals".to_owned(),
            cell: "A1".to_owned(),
            value: None,
            formula: Some("=Data!A1*10".to_owned()),
        }],
    )
    .unwrap();
    let again = read(&saved(&model, &file));
    let snapshot = Excel::snapshot(&again).0;
    assert_eq!(snapshot["sheets"][1]["name"], "Totals");
}

#[test]
fn an_old_change_to_the_same_cell_is_refused_with_the_name() {
    let file = excel_file();
    let mut model = read(&file);
    let earlier = Ordered {
        rev: 1,
        author: "Ana".to_owned(),
        client: "ana".to_owned(),
        seq: 1,
        change: Change::Cells {
            sheet: "1".to_owned(),
            cells: vec![CellChange {
                row: 1,
                column: 0,
                value: Some(Some(Value::Number(1.0))),
                ..CellChange::default()
            }],
        },
    };
    let same = Change::Ops {
        ops: vec![set("A2", Value::Number(2.0))],
    };
    let result = Excel::apply(
        &mut model,
        same,
        std::slice::from_ref(&earlier),
        &mut Assets::default(),
    );
    match result {
        Err(Refused::Behind(detail)) => {
            assert!(detail.contains("Ana"));
            assert!(detail.contains("Data!A2"));
        }
        other => panic!("{other:?}"),
    }
    let other = Change::Ops {
        ops: vec![set("A1", Value::Number(2.0))],
    };
    assert!(Excel::apply(&mut model, other, &[earlier], &mut Assets::default()).is_ok());
}

#[test]
fn a_view_change_names_a_key_and_the_last_one_wins() {
    let file = excel_file();
    let mut model = read(&file);
    let change = |value: f64| Change::Cells {
        sheet: "1".to_owned(),
        cells: vec![CellChange {
            row: 1,
            column: 0,
            value: Some(Some(Value::Number(value))),
            ..CellChange::default()
        }],
    };
    let older = Ordered {
        rev: 1,
        author: "Ana".to_owned(),
        client: "ana".to_owned(),
        seq: 1,
        change: change(1.0),
    };
    Excel::apply(&mut model, change(1.0), &[], &mut Assets::default()).unwrap();
    Excel::apply(&mut model, change(2.0), &[older], &mut Assets::default()).unwrap();
    assert_eq!(
        model.sheet("1").unwrap().cells[&(1, 0)].value,
        Some(Value::Number(2.0))
    );
}

#[test]
fn a_cell_change_is_behind_when_rows_moved() {
    let file = excel_file();
    let mut model = read(&file);
    let moved = Ordered {
        rev: 1,
        author: "Ana".to_owned(),
        client: "ana".to_owned(),
        seq: 1,
        change: Change::Rows {
            sheet: "1".to_owned(),
            at: 0,
            count: 1,
        },
    };
    let change = Change::Cells {
        sheet: "1".to_owned(),
        cells: Vec::new(),
    };
    assert!(matches!(
        Excel::apply(&mut model, change, &[moved], &mut Assets::default()),
        Err(Refused::Behind(_))
    ));
}

#[test]
fn a_group_of_cells_is_behind_when_rows_moved() {
    let file = excel_file();
    let mut model = read(&file);
    let moved = Change::Rows {
        sheet: "1".to_owned(),
        at: 0,
        count: 1,
    };
    let since = [Ordered {
        rev: 1,
        author: "Ana".to_owned(),
        client: "ana".to_owned(),
        seq: 1,
        change: Change::Group {
            changes: vec![moved],
        },
    }];
    let group = Change::Group {
        changes: vec![Change::Cells {
            sheet: "1".to_owned(),
            cells: vec![CellChange {
                row: 0,
                column: 0,
                value: Some(Some(Value::Number(1.0))),
                ..CellChange::default()
            }],
        }],
    };
    assert!(matches!(
        Excel::apply(&mut model, group, &since, &mut Assets::default()),
        Err(Refused::Behind(_))
    ));
}

#[test]
fn a_compare_finds_only_what_a_write_changed() {
    let file = excel_file();
    let base = read(&file);
    let mut other = read(&file);
    ops(&mut other, vec![set("B2", Value::Number(9.0))]).unwrap();
    let written = saved(&other, &file);
    let changes = Excel::compare(&base, &read(&written));
    let cells = changes.iter().map(|change| match change {
        Change::Cells { cells, .. } => cells.len(),
        _ => 0,
    });
    assert_eq!(cells.sum::<usize>(), 1);
}

#[test]
fn check_lists_formula_errors() {
    let file = excel_file();
    let mut model = read(&file);
    ops(
        &mut model,
        vec![Op::SetCell {
            sheet: "Data".to_owned(),
            cell: "D1".to_owned(),
            value: None,
            formula: Some("=1/0".to_owned()),
        }],
    )
    .unwrap();
    let answer = Excel::query(&model, Query::Check).0;
    assert_eq!(answer["errors"][0]["cell"], "D1");
}

#[test]
fn the_guide_names_each_op_that_an_agent_can_send() {
    let guide = Excel::guide();
    for op in &guide.ops {
        let mut example = json!({ "op": op.name });
        for field in op.fields.iter().filter(|field| field.required) {
            example[&field.name] = match field.kind.as_str() {
                "number" => json!(1),
                "rows of values" => json!([[1]]),
                "look" => json!({ "bold": true }),
                _ => json!("x"),
            };
        }
        let parsed: Result<Op, _> = serde_json::from_value(example);
        assert!(parsed.is_ok(), "{}: {parsed:?}", op.name);
    }
}
