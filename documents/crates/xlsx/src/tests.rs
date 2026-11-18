use std::{collections::BTreeMap, io::Write as _, time::Duration};

use document::{Assets, Format, Ordered, Refused};
use package::Package;
use serde_json::json;

use crate::{Change, Model, Op, Query, Value, Xlsx, model::CellChange};

const SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:B4"/><sheetData><row r="1" spans="1:2" x14ac:custom="kept"><c r="A1" s="1"><v>2</v></c><c r="B1" t="s"><v>0</v></c></row><row r="2"><c r="A2"><v>3</v></c></row><row r="3"><c r="A3"><f>A1+A2</f><v>5</v></c><c r="B3"><f t="shared" ref="B3:B4" si="0">A3*2</f><v>10</v></c></row><row r="4"><c r="B4"><f t="shared" si="0"/><v>0</v></c></row></sheetData><mergeCells count="1"><mergeCell ref="A9:B9"/></mergeCells></worksheet>"#;
const WORKBOOK: &str = r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
const RELATIONS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/></Relationships>"#;
const STRINGS: &str = r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>hello</t></si></sst>"#;
const STYLES: &str = r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="12"/><color rgb="FFFF0000"/><name val="Arial"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border/></borders><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0"/></cellXfs></styleSheet>"#;
const TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>"#;
const ROOT_RELATIONS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

fn xlsx_file() -> Vec<u8> {
    xlsx_file_with(
        &SHEET.replace(" x14ac:custom=\"kept\"", " ht=\"20\""),
        WORKBOOK,
        STYLES,
    )
}

fn xlsx_file_with(sheet: &str, workbook: &str, styles: &str) -> Vec<u8> {
    xlsx_package(&[
        ("[Content_Types].xml", TYPES),
        ("_rels/.rels", ROOT_RELATIONS),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", RELATIONS),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/sharedStrings.xml", STRINGS),
        ("xl/styles.xml", styles),
    ])
}

fn xlsx_package(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in parts {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn read(file: &[u8]) -> Model {
    Xlsx::read(file, &mut Assets::default()).unwrap()
}

fn part(file: &[u8], name: &str) -> String {
    Package::open(file).unwrap().text(name).unwrap()
}

fn ops(model: &mut Model, ops: Vec<Op>) -> Result<Change, Refused> {
    Xlsx::apply(model, Change::Ops { ops }, &[], &mut Assets::default())
}

fn saved(model: &Model, file: &[u8]) -> Vec<u8> {
    Xlsx::write(model, file, &Assets::default()).unwrap()
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
    Xlsx::query(
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
    let file = xlsx_file();
    let model = read(&file);
    assert_eq!(saved(&model, &file), file);
}

#[test]
fn cells_come_with_values_formulas_and_looks() {
    let model = read(&xlsx_file());
    let snapshot = Xlsx::snapshot(&model).0;
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
    let file = xlsx_file();
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
    assert!(xml.contains(r#"<c r="A3"><f>A1+A2</f><v>12</v></c>"#));
    assert!(xml.contains(r#"<c r="B3"><f t="shared" ref="B3:B4" si="0">A3*2</f><v>24</v></c>"#));
}

fn set_formula(cell: &str, formula: &str) -> Op {
    Op::SetCell {
        sheet: "Data".to_owned(),
        cell: cell.to_owned(),
        value: None,
        formula: Some(formula.to_owned()),
    }
}

fn formula_at(model: &Model, row: usize, column: usize) -> serde_json::Value {
    Xlsx::snapshot(model).0["sheets"][0]["cells"][row.to_string()][column.to_string()]["formula"]
        .clone()
}

#[test]
fn a_change_to_the_cell_that_holds_a_shared_formula_leaves_the_others_their_formula() {
    let file = xlsx_file();
    let mut model = read(&file);
    let bold = crate::model::Look {
        bold: true,
        ..crate::model::Look::default()
    };
    format(&mut model, "B3:B3", bold);
    let styled = saved(&model, &file);
    let xml = part(&styled, "xl/worksheets/sheet1.xml");
    assert!(xml.contains(r#"<f t="shared" ref="B3:B4" si="0">A3*2</f>"#));
    assert_eq!(formula_at(&read(&styled), 3, 1), "A4*2");
    ops(&mut model, vec![set_formula("B3", "A3*3")]).unwrap();
    let changed = saved(&model, &styled);
    assert!(!part(&changed, "xl/worksheets/sheet1.xml").contains("shared"));
    let again = read(&changed);
    assert_eq!(formula_at(&again, 2, 1), "A3*3");
    assert_eq!(formula_at(&again, 3, 1), "A4*2");
}

#[test]
fn a_new_formula_in_a_cell_that_shares_one_reaches_the_file() {
    let file = xlsx_file();
    let mut model = read(&file);
    ops(&mut model, vec![set_formula("B4", "A4*5")]).unwrap();
    let again = read(&saved(&model, &file));
    assert_eq!(formula_at(&again, 3, 1), "A4*5");
    assert_eq!(formula_at(&again, 2, 1), "A3*2");
}

#[test]
fn a_save_keeps_the_stored_results_that_an_edit_does_not_change() {
    let sheet = SHEET.replace(" x14ac:custom=\"kept\"", "").replace(
        r#"<c r="B1" t="s"><v>0</v></c>"#,
        r#"<c r="B1" t="s"><v>0</v></c><c r="C1"><f>0.1+0.2-0.3</f><v>0</v></c><c r="D1" t="str"><f>""</f><v></v></c>"#,
    );
    let file = xlsx_file_with(&sheet, WORKBOOK, STYLES);
    let mut model = read(&file);
    ops(&mut model, vec![set("A2", Value::Number(10.0))]).unwrap();
    let xml = part(&saved(&model, &file), "xl/worksheets/sheet1.xml");
    assert!(xml.contains(
        r#"<c r="C1"><f>0.1+0.2-0.3</f><v>0</v></c><c r="D1" t="str"><f>""</f><v></v></c>"#
    ));
    assert!(xml.contains(r#"<c r="A3"><f>A1+A2</f><v>12</v></c>"#));
}

#[test]
fn a_save_that_cannot_work_out_the_formulas_asks_the_program_that_opens_it_to() {
    let sheet = SHEET.replace(" x14ac:custom=\"kept\"", "");
    for (before, asking) in [
        ("", r#"<calcPr fullCalcOnLoad="1"/>"#),
        (
            r#"<calcPr calcId="191029"/>"#,
            r#"<calcPr fullCalcOnLoad="1" calcId="191029"/>"#,
        ),
    ] {
        let workbook = WORKBOOK.replace("</sheets>", &format!("</sheets>{before}"));
        let file = xlsx_file_with(&sheet, &workbook, STYLES);
        let mut model = read(&file);
        ops(&mut model, vec![set("A2", Value::Number(10.0))]).unwrap();
        let unfinished = crate::write::write(&model, &file, Duration::ZERO).unwrap();
        assert!(
            part(&unfinished, "xl/workbook.xml").contains(&format!("</sheets>{asking}</workbook>"))
        );
        assert!(
            part(&unfinished, "xl/worksheets/sheet1.xml").contains(r#"<c r="A3"><f>A1+A2</f></c>"#)
        );
        let finished = saved(&model, &unfinished);
        assert!(
            part(&finished, "xl/workbook.xml").contains(&format!("</sheets>{before}</workbook>"))
        );
        assert!(
            part(&finished, "xl/worksheets/sheet1.xml")
                .contains(r#"<c r="A3"><f>A1+A2</f><v>12</v></c>"#)
        );
    }
}

#[test]
fn a_workbook_asks_to_be_worked_out_while_a_formula_keeps_its_value() {
    let sheet = SHEET.replace(" x14ac:custom=\"kept\"", "").replace(
        r#"<c r="B1" t="s"><v>0</v></c>"#,
        r#"<c r="B1" t="s"><v>0</v></c><c r="D1"><f>KODOSIUNKNOWN(A2)</f><v>7</v></c>"#,
    );
    let workbook = WORKBOOK.replace("</sheets>", r#"</sheets><calcPr fullCalcOnLoad="1"/>"#);
    let file = xlsx_file_with(&sheet, &workbook, STYLES);
    let mut model = read(&file);
    ops(&mut model, vec![set("A2", Value::Number(10.0))]).unwrap();
    assert!(part(&saved(&model, &file), "xl/workbook.xml").contains("fullCalcOnLoad"));
}

#[test]
fn an_answer_says_when_the_formulas_could_not_be_worked_out_in_time() {
    let file = xlsx_file();
    let mut model = read(&file);
    ops(&mut model, vec![set("A2", Value::Number(10.0))]).unwrap();
    let everything = Query::Read {
        sheet: None,
        range: None,
    };
    let unfinished = crate::ops::answer(&model, &everything, Duration::ZERO);
    let formula = unfinished["sheets"][0]["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|cell| cell["cell"] == "A3")
        .unwrap();
    assert_eq!(formula["value"], serde_json::Value::Null);
    assert!(unfinished["problem"].is_string());
    let unchecked = crate::ops::answer(&model, &Query::Check, Duration::ZERO);
    assert!(unchecked.get("errors").is_none() && unchecked["problem"].is_string());
    assert!(Xlsx::query(&model, everything).0.get("problem").is_none());
    assert!(Xlsx::query(&model, Query::Check).0.get("problem").is_none());
}

#[test]
fn a_read_sees_results_before_any_save() {
    let file = xlsx_file();
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
    let mut model = read(&xlsx_file_with(&sheet, &workbook, STYLES));
    ops(&mut model, vec![set("A2", Value::Number(10.0))]).unwrap();
    assert_eq!(cell_value(&model, "C1"), 22.0);
    assert_eq!(cell_value(&model, "D1"), 7.0);
}

#[test]
fn a_workbook_that_counts_days_from_1904_gets_a_date_in_its_count() {
    let sheet = SHEET.replace(" x14ac:custom=\"kept\"", "").replace(
        r#"<c r="B1" t="s"><v>0</v></c>"#,
        r#"<c r="B1" t="s"><v>0</v></c><c r="C1"><f>DATE(2020,1,1)</f><v>0</v></c>"#,
    );
    let workbook = WORKBOOK.replace("<sheets>", r#"<workbookPr date1904="1"/><sheets>"#);
    let mut model = read(&xlsx_file_with(&sheet, &workbook, STYLES));
    ops(&mut model, vec![set("A2", Value::Number(10.0))]).unwrap();
    assert_eq!(cell_value(&model, "C1"), 42369.0);
}

#[test]
fn formulas_in_a_loop_keep_their_values() {
    let sheet = SHEET.replace(" x14ac:custom=\"kept\"", "").replace(
        r#"<c r="B1" t="s"><v>0</v></c>"#,
        r#"<c r="B1" t="s"><v>0</v></c><c r="C1"><f>D1</f><v>7</v></c><c r="D1"><f>C1</f><v>7</v></c>"#,
    );
    let mut model = read(&xlsx_file_with(&sheet, WORKBOOK, STYLES));
    ops(&mut model, vec![set("A2", Value::Number(10.0))]).unwrap();
    assert_eq!(cell_value(&model, "C1"), 7.0);
}

#[test]
fn new_text_keeps_the_shared_strings_as_they_are() {
    let file = xlsx_file();
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
fn a_cell_goes_into_a_sheet_that_has_no_rows() {
    let empty = r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1"/><sheetData/></worksheet>"#;
    let file = xlsx_file_with(empty, WORKBOOK, STYLES);
    let mut model = read(&file);
    ops(&mut model, vec![set("B2", Value::Number(10.0))]).unwrap();
    let xml = part(&saved(&model, &file), "xl/worksheets/sheet1.xml");
    assert!(xml.contains(r#"<sheetData><row r="2"><c r="B2"><v>10</v></c></row></sheetData>"#));
}

#[test]
fn a_workbook_saves_with_its_chart_sheet_as_it_is() {
    let chart_sheet = r#"<?xml version="1.0"?><chartsheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetViews><sheetView workbookViewId="0"/></sheetViews><drawing r:id="rId1"/></chartsheet>"#;
    let workbook = WORKBOOK.replace(
        "</sheets>",
        r#"<sheet name="Chart" sheetId="2" r:id="rId4"/></sheets>"#,
    );
    let relations = RELATIONS.replace(
        "</Relationships>",
        r#"<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartsheet" Target="chartsheets/sheet1.xml"/></Relationships>"#,
    );
    let file = xlsx_package(&[
        ("[Content_Types].xml", TYPES),
        ("_rels/.rels", ROOT_RELATIONS),
        ("xl/workbook.xml", &workbook),
        ("xl/_rels/workbook.xml.rels", &relations),
        (
            "xl/worksheets/sheet1.xml",
            &SHEET.replace(" x14ac:custom=\"kept\"", ""),
        ),
        ("xl/chartsheets/sheet1.xml", chart_sheet),
        ("xl/sharedStrings.xml", STRINGS),
        ("xl/styles.xml", STYLES),
    ]);
    let mut model = read(&file);
    ops(&mut model, vec![set("A2", Value::Number(10.0))]).unwrap();
    let new_file = saved(&model, &file);
    assert!(part(&new_file, "xl/worksheets/sheet1.xml").contains(r#"<c r="A2"><v>10</v></c>"#));
    assert_eq!(part(&new_file, "xl/chartsheets/sheet1.xml"), chart_sheet);
    for op in [
        Op::SetCell {
            sheet: "Chart".to_owned(),
            cell: "A1".to_owned(),
            value: Some(Value::Number(1.0)),
            formula: None,
        },
        Op::InsertRows {
            sheet: "Chart".to_owned(),
            at: 0,
            count: 1,
        },
    ] {
        assert!(matches!(
            ops(&mut model, vec![op]),
            Err(Refused::Invalid(_))
        ));
    }
}

#[test]
fn a_new_row_goes_in_its_place() {
    let file = xlsx_file();
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
    let file = xlsx_file();
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
    let file = xlsx_file();
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
    let snapshot = Xlsx::snapshot(&again).0;
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
    let mut model = read(&xlsx_file());
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
    let sent = Xlsx::apply(&mut model, change, &[], &mut Assets::default()).unwrap();
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
    let file = xlsx_file();
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
    let snapshot = Xlsx::snapshot(&read(&second)).0;
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
    let file = xlsx_file_with(
        &SHEET.replace(" x14ac:custom=\"kept\"", ""),
        WORKBOOK,
        &styles,
    );
    let mut model = read(&file);
    assert_eq!(
        Xlsx::snapshot(&model).0["looks"][1]["numberFormat"],
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
    let file = xlsx_file();
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
    let snapshot = Xlsx::snapshot(&again).0;
    assert_eq!(snapshot["sheets"][0]["cells"]["4"]["0"]["formula"], "A1+A4");
    assert_eq!(cell_value(&again, "A5"), 5.0);
    assert_eq!(cell_value(&again, "A4"), 3.0);
}

#[test]
fn sizes_and_merges_reach_the_file() {
    let file = xlsx_file();
    let mut model = read(&file);
    let mut apply = |change| Xlsx::apply(&mut model, change, &[], &mut Assets::default());
    apply(Change::Size {
        sheet: "1".to_owned(),
        widths: [(1, 30.0)].into(),
        heights: [(1, Some(44.0)), (9, Some(12.0))].into(),
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
    let snapshot = Xlsx::snapshot(&again).0;
    assert_eq!(snapshot["sheets"][0]["widths"]["1"], 30.0);
    assert_eq!(snapshot["sheets"][0]["heights"]["1"], 44.0);
    assert_eq!(snapshot["sheets"][0]["merges"], json!([[0, 2, 1, 3]]));
}

#[test]
fn a_place_moves_with_its_rows_and_reads_as_a_range() {
    let mut model = read(&xlsx_file());
    let mut place = json!({
        "sheet": model.sheets[0].id,
        "row": 2,
        "column": 1,
        "endRow": 3,
        "endColumn": 1,
        "editing": true,
    });
    let inserted = Op::InsertRows {
        sheet: "Data".to_owned(),
        at: 1,
        count: 2,
    };
    let change = ops(&mut model, vec![inserted]).unwrap();
    Xlsx::map_place(&mut place, &change);
    assert_eq!(
        (place["row"].clone(), place["endRow"].clone()),
        (json!(4), json!(5))
    );
    assert_eq!(
        Xlsx::describe_place(&model, &place),
        json!({ "range": "Data!B5:B6", "editing": true })
    );
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
    let file = xlsx_file_with(&sheet, &workbook, STYLES);
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
    assert_eq!(Xlsx::snapshot(&again).0["sheets"][0]["heights"]["3"], 33.0);
}

#[test]
fn a_sheet_from_a_view_keeps_its_cells_across_saves() {
    let file = xlsx_file();
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
        Xlsx::apply(model, change, &[], &mut Assets::default()).unwrap();
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
    let snapshot = Xlsx::snapshot(&read(&second)).0;
    assert_eq!(snapshot["sheets"].as_array().unwrap().len(), 2);
    assert_eq!(snapshot["sheets"][1]["name"], "Plan");
    assert_eq!(snapshot["sheets"][1]["cells"]["0"]["0"]["value"], 1.0);
    assert_eq!(snapshot["sheets"][1]["cells"]["1"]["0"]["value"], 2.0);
}

#[test]
fn a_removed_sheet_leaves_no_part_behind() {
    let file = xlsx_file();
    let mut model = read(&file);
    let apply = |model: &mut Model, change| {
        Xlsx::apply(model, change, &[], &mut Assets::default()).unwrap();
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
    let file = xlsx_file();
    let mut model = read(&file);
    let mut apply = |change| {
        Xlsx::apply(&mut model, change, &[], &mut Assets::default()).unwrap();
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
    let snapshot = Xlsx::snapshot(&read(&saved(&model, &file))).0;
    let names = snapshot["sheets"].as_array().unwrap();
    assert_eq!(names.len(), 2);
    assert_eq!(names[0]["name"], "First");
    assert_eq!(names[1]["name"], "Second");
    assert_eq!(names[0]["cells"], json!({}));
}

#[test]
fn a_new_sheet_goes_in_the_package() {
    let file = xlsx_file();
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
    let snapshot = Xlsx::snapshot(&again).0;
    assert_eq!(snapshot["sheets"][1]["name"], "Totals");
}

#[test]
fn an_old_change_to_the_same_cell_is_refused_with_the_name() {
    let file = xlsx_file();
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
    let result = Xlsx::apply(
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
    assert!(Xlsx::apply(&mut model, other, &[earlier], &mut Assets::default()).is_ok());
}

#[test]
fn a_view_change_names_a_key_and_the_last_one_wins() {
    let file = xlsx_file();
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
    Xlsx::apply(&mut model, change(1.0), &[], &mut Assets::default()).unwrap();
    Xlsx::apply(&mut model, change(2.0), &[older], &mut Assets::default()).unwrap();
    assert_eq!(
        model.sheet("1").unwrap().cells[&(1, 0)].value,
        Some(Value::Number(2.0))
    );
}

#[test]
fn a_cell_change_is_behind_when_rows_moved() {
    let file = xlsx_file();
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
        Xlsx::apply(&mut model, change, &[moved], &mut Assets::default()),
        Err(Refused::Behind(_))
    ));
}

#[test]
fn a_group_of_cells_is_behind_when_rows_moved() {
    let file = xlsx_file();
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
        Xlsx::apply(&mut model, group, &since, &mut Assets::default()),
        Err(Refused::Behind(_))
    ));
}

#[test]
fn a_compare_finds_only_what_a_write_changed() {
    let file = xlsx_file();
    let base = read(&file);
    let mut other = read(&file);
    ops(&mut other, vec![set("B2", Value::Number(9.0))]).unwrap();
    let written = saved(&other, &file);
    let changes = Xlsx::compare(&base, &read(&written));
    let cells = changes.iter().map(|change| match change {
        Change::Cells { cells, .. } => cells.len(),
        _ => 0,
    });
    assert_eq!(cells.sum::<usize>(), 1);
}

#[test]
fn check_lists_formula_errors() {
    let file = xlsx_file();
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
    let answer = Xlsx::query(&model, Query::Check).0;
    assert_eq!(answer["errors"][0]["cell"], "D1");
}

#[test]
fn the_guide_names_each_op_that_an_agent_can_send() {
    let guide = Xlsx::guide();
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

#[test]
fn a_link_of_a_cell_says_where_it_goes_and_moves_with_its_row() {
    let sheet = SHEET
        .replace(" x14ac:custom=\"kept\"", " ht=\"20\"")
        .replace(
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
        )
        .replace(
            "</mergeCells>",
            r#"</mergeCells><hyperlinks><hyperlink ref="A1" r:id="rId1"/><hyperlink ref="B2" location="'Data'!A3" display="Totals"/></hyperlinks>"#,
        );
    let file = xlsx_file_with(&sheet, WORKBOOK, STYLES);
    let mut parts = Package::open(&file).unwrap();
    let names: Vec<String> = parts.names().map(str::to_owned).collect();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for name in names {
        writer.start_file(name.as_str(), options).unwrap();
        writer.write_all(&parts.bytes(&name).unwrap()).unwrap();
    }
    writer
        .start_file("xl/worksheets/_rels/sheet1.xml.rels", options)
        .unwrap();
    writer
        .write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://kodosi.com/docs" TargetMode="External"/></Relationships>"#)
        .unwrap();
    let file = writer.finish().unwrap().into_inner();

    let model = read(&file);
    assert_eq!(
        model.snapshot()["sheets"][0]["links"],
        json!([
            { "ranges": "A1", "to": { "href": "https://kodosi.com/docs" } },
            { "ranges": "B2", "to": { "location": "'Data'!A3" } },
        ])
    );
    assert_eq!(saved(&model, &file), file);

    let mut moved = model;
    ops(
        &mut moved,
        vec![Op::InsertRows {
            sheet: "1".to_owned(),
            at: 0,
            count: 1,
        }],
    )
    .unwrap();
    let links = &moved.snapshot()["sheets"][0]["links"];
    assert_eq!(
        (&links[0]["ranges"], &links[1]["ranges"]),
        (&json!("A2"), &json!("B3"))
    );
}

fn change(model: &mut Model, change: Change) {
    Xlsx::apply(model, change, &[], &mut Assets::default()).unwrap();
}

fn freeze(top: u32, left: u32, rows: u32, columns: u32) -> Change {
    Change::Freeze {
        sheet: "1".to_owned(),
        top,
        left,
        rows,
        columns,
    }
}

fn hide(rows: &[(u32, bool)], columns: &[(u32, bool)]) -> Change {
    Change::Hidden {
        sheet: "1".to_owned(),
        rows: rows.iter().copied().collect(),
        columns: columns.iter().copied().collect(),
    }
}

#[test]
fn hidden_lines_and_frozen_panes_reach_the_file() {
    let file = xlsx_file();
    let mut model = read(&file);
    change(&mut model, hide(&[(1, true), (11, true)], &[(2, true)]));
    change(&mut model, freeze(0, 0, 1, 2));
    let hidden = saved(&model, &file);
    let xml = part(&hidden, "xl/worksheets/sheet1.xml");
    assert!(xml.contains(r#"<sheetViews><sheetView workbookViewId="0"><pane xSplit="2" ySplit="1" topLeftCell="C2" activePane="bottomRight" state="frozen"/></sheetView></sheetViews><cols><col min="3" max="3" hidden="1"/></cols><sheetData>"#));
    assert!(xml.contains(r#"<row r="2" hidden="1">"#));
    assert!(xml.contains(r#"<row r="12" hidden="1"></row>"#));
    let again = read(&hidden);
    assert_eq!(saved(&again, &hidden), hidden);
    let sheet = &Xlsx::snapshot(&again).0["sheets"][0];
    assert_eq!(sheet["hiddenRows"], json!([1, 11]));
    assert_eq!(sheet["hiddenColumns"], json!([2]));
    assert_eq!(
        sheet["frozen"],
        json!({ "top": 0, "left": 0, "rows": 1, "columns": 2 })
    );
    assert_eq!(
        Xlsx::compare(&read(&file), &again),
        vec![
            hide(&[(1, true), (11, true)], &[(2, true)]),
            freeze(0, 0, 1, 2)
        ]
    );

    change(&mut model, hide(&[(1, false), (11, false)], &[(2, false)]));
    change(&mut model, freeze(0, 0, 0, 0));
    let xml = part(&saved(&model, &hidden), "xl/worksheets/sheet1.xml");
    assert!(!xml.contains("hidden") && !xml.contains("<cols") && !xml.contains("<pane"));
}

#[test]
fn a_file_keeps_its_hidden_lines_and_frozen_panes_and_they_move_with_its_rows() {
    let sheet = SHEET
        .replace(" x14ac:custom=\"kept\"", "")
        .replace(r#"<row r="2">"#, r#"<row r="2" hidden="1">"#)
        .replace(
            "<sheetData>",
            r#"<sheetViews><sheetView tabSelected="1" topLeftCell="A3" workbookViewId="0"><pane ySplit="1" topLeftCell="A4" activePane="bottomLeft" state="frozen"/><selection pane="bottomLeft" activeCell="B5" sqref="B5"/></sheetView></sheetViews><cols><col min="2" max="2" width="12" hidden="1" customWidth="1"/><col min="4" max="5" style="1" hidden="1"/></cols><sheetData>"#,
        );
    let file = xlsx_file_with(&sheet, WORKBOOK, STYLES);
    let mut model = read(&file);
    let snapshot = Xlsx::snapshot(&model).0;
    assert_eq!(snapshot["sheets"][0]["hiddenRows"], json!([1]));
    assert_eq!(snapshot["sheets"][0]["hiddenColumns"], json!([1, 3, 4]));
    assert_eq!(
        snapshot["sheets"][0]["frozen"],
        json!({ "top": 2, "left": 0, "rows": 1, "columns": 0 })
    );
    ops(&mut model, vec![set("A1", Value::Number(1.0))]).unwrap();
    let edited = part(&saved(&model, &file), "xl/worksheets/sheet1.xml");
    assert!(edited.contains(r#"<row r="2" hidden="1">"#));
    assert!(edited.contains(r#"<pane ySplit="1" topLeftCell="A4" activePane="bottomLeft" state="frozen"/><selection pane="bottomLeft" activeCell="B5" sqref="B5"/>"#));

    ops(
        &mut model,
        vec![
            Op::InsertRows {
                sheet: "Data".to_owned(),
                at: 0,
                count: 2,
            },
            Op::DeleteColumns {
                sheet: "Data".to_owned(),
                at: 2,
                count: 1,
            },
        ],
    )
    .unwrap();
    let moved = saved(&model, &file);
    let xml = part(&moved, "xl/worksheets/sheet1.xml");
    assert!(xml.contains(r#"<sheetView tabSelected="1" workbookViewId="0" topLeftCell="A3"><pane ySplit="3" topLeftCell="A6" activePane="bottomLeft" state="frozen"/></sheetView>"#));
    assert!(xml.contains(r#"<row r="4" hidden="1">"#));
    assert!(xml.contains(r#"<col min="2" max="2" width="12" customWidth="1" hidden="1"/><col min="3" max="3" style="1" hidden="1"/><col min="4" max="4" style="1" hidden="1"/>"#));
    let sheet = &Xlsx::snapshot(&read(&moved)).0["sheets"][0];
    assert_eq!(sheet["hiddenRows"], json!([3]));
    assert_eq!(sheet["hiddenColumns"], json!([1, 2, 3]));
}

#[test]
fn lines_that_come_or_go_before_frozen_panes_move_them() {
    let mut model = read(&xlsx_file());
    let frozen = |model: &Model| Xlsx::snapshot(model).0["sheets"][0]["frozen"].clone();
    let rows = |at, count| Change::Rows {
        sheet: "1".to_owned(),
        at,
        count,
    };
    change(&mut model, freeze(3, 1, 2, 1));
    change(&mut model, rows(5, 4));
    assert_eq!(
        frozen(&model),
        json!({ "top": 3, "left": 1, "rows": 2, "columns": 1 })
    );
    change(&mut model, rows(4, 2));
    assert_eq!(
        frozen(&model),
        json!({ "top": 3, "left": 1, "rows": 4, "columns": 1 })
    );
    change(&mut model, rows(0, -2));
    assert_eq!(
        frozen(&model),
        json!({ "top": 3, "left": 1, "rows": 2, "columns": 1 })
    );
    change(&mut model, rows(2, -10));
    assert_eq!(
        frozen(&model),
        json!({ "top": 1, "left": 1, "rows": 1, "columns": 1 })
    );
    change(
        &mut model,
        Change::Columns {
            sheet: "1".to_owned(),
            at: 0,
            count: 3,
        },
    );
    assert_eq!(
        frozen(&model),
        json!({ "top": 1, "left": 1, "rows": 1, "columns": 4 })
    );
}

#[test]
fn a_new_sheet_saves_its_panes_and_columns_at_once() {
    let file = xlsx_file();
    let mut model = read(&file);
    change(
        &mut model,
        Change::AddSheet {
            id: "plan".to_owned(),
            name: "Plan".to_owned(),
        },
    );
    for change_of_plan in [
        Change::Size {
            sheet: "plan".to_owned(),
            widths: [(0, 20.0)].into(),
            heights: BTreeMap::new(),
        },
        Change::Hidden {
            sheet: "plan".to_owned(),
            rows: [(4, true)].into(),
            columns: [(1, true)].into(),
        },
        Change::Freeze {
            sheet: "plan".to_owned(),
            top: 0,
            left: 0,
            rows: 1,
            columns: 0,
        },
    ] {
        change(&mut model, change_of_plan);
    }
    let xml = part(&saved(&model, &file), "xl/worksheets/sheetN2.xml");
    assert!(
        xml.contains(
            r#"<pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/>"#
        )
    );
    assert!(xml.contains(r#"<cols><col min="1" max="1" width="20" customWidth="1"/><col min="2" max="2" hidden="1"/></cols>"#));
    assert!(xml.contains(r#"<row r="5" hidden="1">"#));
}

#[test]
fn lines_and_panes_past_the_last_line_are_refused() {
    let mut model = read(&xlsx_file());
    for refused in [
        hide(&[(1_048_576, true)], &[]),
        hide(&[], &[(16_384, true)]),
        freeze(1_048_575, 0, 1, 0),
        freeze(0, 16_380, 0, 4),
    ] {
        assert!(matches!(
            Xlsx::apply(&mut model, refused, &[], &mut Assets::default()),
            Err(Refused::Invalid(_))
        ));
    }
    change(&mut model, hide(&[(1_048_575, true)], &[(16_383, true)]));
}

#[test]
fn a_view_names_lines_by_their_number_as_text() {
    let mut model = read(&xlsx_file());
    let batch = serde_json::from_value(json!({
        "kind": "group",
        "changes": [
            { "kind": "size", "sheet": "1", "widths": { "1": 30 }, "heights": { "0": null, "4": 22.5 } },
            { "kind": "hidden", "sheet": "1", "rows": { "4": true }, "columns": { "2": true } },
        ],
    }))
    .unwrap();
    change(&mut model, batch);
    let sheet = &Xlsx::snapshot(&model).0["sheets"][0];
    assert_eq!(
        (&sheet["widths"], &sheet["heights"]),
        (&json!({ "1": 30.0 }), &json!({ "4": 22.5 }))
    );
    assert_eq!(
        (&sheet["hiddenRows"], &sheet["hiddenColumns"]),
        (&json!([4]), &json!([2]))
    );
}

#[test]
fn a_row_that_fits_what_it_holds_loses_its_height_in_the_file() {
    let file = xlsx_file();
    let mut model = read(&file);
    change(
        &mut model,
        Change::Size {
            sheet: "1".to_owned(),
            widths: BTreeMap::new(),
            heights: [(0, None)].into(),
        },
    );
    let fitted = saved(&model, &file);
    assert!(part(&fitted, "xl/worksheets/sheet1.xml").contains(r#"<row r="1" spans="1:2">"#));
    assert_eq!(
        Xlsx::compare(&read(&file), &read(&fitted)),
        vec![Change::Size {
            sheet: "1".to_owned(),
            widths: BTreeMap::new(),
            heights: [(0, None)].into(),
        }]
    );
}

#[test]
fn a_cell_that_a_view_empties_goes_from_the_file_and_from_the_others() {
    let file = xlsx_file();
    let mut model = read(&file);
    let cleared = serde_json::from_value(json!({
        "kind": "cells",
        "sheet": "1",
        "cells": [{ "row": 1, "column": 0, "value": null, "formula": null, "style": null }],
    }))
    .unwrap();
    let sent = Xlsx::apply(&mut model, cleared, &[], &mut Assets::default()).unwrap();
    assert_eq!(
        serde_json::to_value(&sent).unwrap()["cells"][0],
        json!({ "row": 1, "column": 0, "value": null, "formula": null, "style": null })
    );
    assert_eq!(
        cell_value(&read(&saved(&model, &file)), "A2"),
        serde_json::Value::Null
    );
}
