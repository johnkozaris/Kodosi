use document::{Guide, Ordered, Refused, guide_field as field, guide_op as op};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::{
    calculate::calculated,
    formula::{cell_name, parse_cell, parse_range},
    model::{CellChange, Change, Look, Model, Query, Sheet, Value},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    SetCell {
        sheet: String,
        cell: String,
        #[serde(default)]
        value: Option<Value>,
        #[serde(default)]
        formula: Option<String>,
    },
    SetRange {
        sheet: String,
        start: String,
        rows: Vec<Vec<Option<Value>>>,
    },
    ClearRange {
        sheet: String,
        range: String,
    },
    FormatRange {
        sheet: String,
        range: String,
        look: Look,
    },
    InsertRows {
        sheet: String,
        at: u32,
        count: u32,
    },
    DeleteRows {
        sheet: String,
        at: u32,
        count: u32,
    },
    InsertColumns {
        sheet: String,
        at: u32,
        count: u32,
    },
    DeleteColumns {
        sheet: String,
        at: u32,
        count: u32,
    },
    AddSheet {
        name: String,
    },
    RenameSheet {
        sheet: String,
        name: String,
    },
    RemoveSheet {
        sheet: String,
    },
}

pub(crate) fn guide() -> Guide {
    let sheet = || field("sheet", "sheet name", true, "The sheet");
    let lines = |does: &str| {
        vec![
            sheet(),
            field("at", "number", true, "The first line, from 0"),
            field("count", "number", true, does),
        ]
    };
    Guide {
        format: "xlsx".to_owned(),
        ops: vec![
            op(
                "setCell",
                "Sets a cell's value or formula",
                vec![
                    sheet(),
                    field("cell", "cell name", true, "Such as B2"),
                    field("value", "number, text or boolean", false, "Its value"),
                    field(
                        "formula",
                        "formula",
                        false,
                        "Its formula, such as =SUM(A1:A9)",
                    ),
                ],
            ),
            op(
                "setRange",
                "Sets the values of a block of cells",
                vec![
                    sheet(),
                    field("start", "cell name", true, "The top left cell"),
                    field("rows", "rows of values", true, "One list for each row"),
                ],
            ),
            op(
                "clearRange",
                "Empties cells",
                vec![sheet(), field("range", "range", true, "Such as A1:C9")],
            ),
            op(
                "formatRange",
                "Sets the look of cells",
                vec![
                    sheet(),
                    field("range", "range", true, "Such as A1:C9"),
                    field(
                        "look",
                        "look",
                        true,
                        "Any of bold, italic, underline, strike, color, size, font, fill, numberFormat, align, vertical, wrap",
                    ),
                ],
            ),
            op("insertRows", "Adds rows", lines("How many")),
            op("deleteRows", "Removes rows", lines("How many")),
            op("insertColumns", "Adds columns", lines("How many")),
            op("deleteColumns", "Removes columns", lines("How many")),
            op(
                "addSheet",
                "Adds a sheet",
                vec![field("name", "text", true, "Its name")],
            ),
            op(
                "renameSheet",
                "Gives a sheet a new name",
                vec![sheet(), field("name", "text", true, "Its new name")],
            ),
            op("removeSheet", "Removes a sheet", vec![sheet()]),
        ],
    }
}

fn sheet_named<'m>(model: &'m Model, name: &str) -> Result<&'m Sheet, Refused> {
    model
        .sheets
        .iter()
        .find(|sheet| sheet.name == name || sheet.id == name)
        .ok_or_else(|| Refused::Invalid(format!("There is no sheet {name}.")))
}

fn bad_name(name: &str) -> Refused {
    Refused::Invalid(format!("{name} is not a cell or a range"))
}

fn cells_of(op: &Op) -> Vec<(String, u32, u32)> {
    let range = |sheet: &str, [top, left, bottom, right]: [u32; 4]| -> Vec<(String, u32, u32)> {
        (top..=bottom)
            .flat_map(|row| (left..=right).map(move |column| (row, column)))
            .map(|(row, column)| (sheet.to_owned(), row, column))
            .collect()
    };
    match op {
        Op::SetCell { sheet, cell, .. } => parse_cell(cell)
            .map(|(row, column)| (sheet.clone(), row, column))
            .into_iter()
            .collect(),
        Op::SetRange { sheet, start, rows } => parse_cell(start)
            .map(|(top, left)| {
                let width = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
                let bottom = top + u32::try_from(rows.len().max(1) - 1).unwrap_or(0);
                let right = left + u32::try_from(width - 1).unwrap_or(0);
                range(sheet, [top, left, bottom, right])
            })
            .unwrap_or_default(),
        Op::ClearRange { sheet, range: name }
        | Op::FormatRange {
            sheet, range: name, ..
        } => parse_range(name)
            .map(|bounds| range(sheet, bounds))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// An agent is slow, so an op on a cell that someone changed since its read is refused, and the
/// answer says who changed what.
pub(crate) fn check_conflicts(
    ops: &[Op],
    model: &Model,
    since: &[Ordered<Change>],
) -> Result<(), Refused> {
    for ordered in since {
        let mut changed = Vec::new();
        collect_cells(&ordered.change, &mut changed);
        for op in ops {
            for (sheet, row, column) in cells_of(op) {
                let id = sheet_named(model, &sheet)?.id.clone();
                if changed.contains(&(id, row, column)) {
                    return Err(Refused::Behind(format!(
                        "{} changed {sheet}!{} after you read it. Read again.",
                        ordered.author,
                        cell_name(row, column)
                    )));
                }
            }
        }
    }
    Ok(())
}

fn collect_cells(change: &Change, out: &mut Vec<(String, u32, u32)>) {
    match change {
        Change::Cells { sheet, cells } => {
            out.extend(
                cells
                    .iter()
                    .map(|cell| (sheet.clone(), cell.row, cell.column)),
            );
        }
        Change::Group { changes } => {
            for change in changes {
                collect_cells(change, out);
            }
        }
        _ => {}
    }
}

fn signed(count: u32) -> Result<i32, Refused> {
    i32::try_from(count)
        .map_err(|_| Refused::Invalid("There are too many rows or columns.".to_owned()))
}

fn next_sheet_id(model: &Model, taken: u32) -> String {
    let highest = model
        .sheets
        .iter()
        .filter_map(|sheet| sheet.id.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    (highest + taken + 1).to_string()
}

#[expect(clippy::too_many_lines, reason = "one arm for each op")]
pub(crate) fn to_changes(model: &Model, ops: &[Op]) -> Result<Vec<Change>, Refused> {
    let mut changes = Vec::with_capacity(ops.len());
    let mut added = 0;
    for op in ops {
        changes.push(match op {
            Op::SetCell {
                sheet,
                cell,
                value,
                formula,
            } => {
                let (row, column) = parse_cell(cell).ok_or_else(|| bad_name(cell))?;
                let formula = formula
                    .as_ref()
                    .map(|formula| formula.trim_start_matches('=').to_owned());
                Change::Cells {
                    sheet: sheet_named(model, sheet)?.id.clone(),
                    cells: vec![CellChange {
                        row,
                        column,
                        value: Some(value.clone()),
                        formula: Some(formula),
                        ..CellChange::default()
                    }],
                }
            }
            Op::SetRange { sheet, start, rows } => {
                let (top, left) = parse_cell(start).ok_or_else(|| bad_name(start))?;
                let mut cells = Vec::new();
                for (row, values) in (top..).zip(rows) {
                    for (column, value) in (left..).zip(values) {
                        cells.push(CellChange {
                            row,
                            column,
                            value: Some(value.clone()),
                            formula: Some(None),
                            ..CellChange::default()
                        });
                    }
                }
                Change::Cells {
                    sheet: sheet_named(model, sheet)?.id.clone(),
                    cells,
                }
            }
            Op::ClearRange { sheet, range } => {
                let [top, left, bottom, right] =
                    parse_range(range).ok_or_else(|| bad_name(range))?;
                let cells = (top..=bottom)
                    .flat_map(|row| (left..=right).map(move |column| (row, column)))
                    .map(|(row, column)| CellChange {
                        row,
                        column,
                        value: Some(None),
                        formula: Some(None),
                        ..CellChange::default()
                    })
                    .collect();
                Change::Cells {
                    sheet: sheet_named(model, sheet)?.id.clone(),
                    cells,
                }
            }
            Op::FormatRange { sheet, range, look } => {
                let [top, left, bottom, right] =
                    parse_range(range).ok_or_else(|| bad_name(range))?;
                let cells = (top..=bottom)
                    .flat_map(|row| (left..=right).map(move |column| (row, column)))
                    .map(|(row, column)| CellChange {
                        row,
                        column,
                        look: Some(look.clone()),
                        ..CellChange::default()
                    })
                    .collect();
                Change::Cells {
                    sheet: sheet_named(model, sheet)?.id.clone(),
                    cells,
                }
            }
            Op::InsertRows { sheet, at, count } => Change::Rows {
                sheet: sheet_named(model, sheet)?.id.clone(),
                at: *at,
                count: signed(*count)?,
            },
            Op::DeleteRows { sheet, at, count } => Change::Rows {
                sheet: sheet_named(model, sheet)?.id.clone(),
                at: *at,
                count: -signed(*count)?,
            },
            Op::InsertColumns { sheet, at, count } => Change::Columns {
                sheet: sheet_named(model, sheet)?.id.clone(),
                at: *at,
                count: signed(*count)?,
            },
            Op::DeleteColumns { sheet, at, count } => Change::Columns {
                sheet: sheet_named(model, sheet)?.id.clone(),
                at: *at,
                count: -signed(*count)?,
            },
            Op::AddSheet { name } => {
                added += 1;
                Change::AddSheet {
                    id: next_sheet_id(model, added - 1),
                    name: name.clone(),
                }
            }
            Op::RenameSheet { sheet, name } => Change::RenameSheet {
                sheet: sheet_named(model, sheet)?.id.clone(),
                name: name.clone(),
            },
            Op::RemoveSheet { sheet } => Change::RemoveSheet {
                sheet: sheet_named(model, sheet)?.id.clone(),
            },
        });
    }
    Ok(changes)
}

fn sheet_json(sheet: &Sheet, range: Option<&str>) -> Json {
    let bounds = range.and_then(parse_range);
    let rows: Vec<Json> = sheet
        .cells
        .iter()
        .filter(|((row, column), _)| {
            bounds.is_none_or(|[top, left, bottom, right]| {
                (top..=bottom).contains(row) && (left..=right).contains(column)
            })
        })
        .map(|((row, column), cell)| {
            json!({
                "cell": cell_name(*row, *column),
                "value": cell.value,
                "formula": cell.formula,
            })
        })
        .collect();
    json!({ "name": sheet.name, "cells": rows })
}

pub(crate) fn answer(model: &Model, query: &Query) -> Json {
    let current = calculated(model);
    match query {
        Query::Read { sheet, range } => {
            let sheets: Vec<Json> = current
                .sheets
                .iter()
                .filter(|known| {
                    sheet
                        .as_deref()
                        .is_none_or(|sheet| known.name == sheet || known.id == sheet)
                })
                .map(|sheet| sheet_json(sheet, range.as_deref()))
                .collect();
            json!({ "sheets": sheets })
        }
        Query::Check => {
            let errors: Vec<Json> = current
                .sheets
                .iter()
                .flat_map(|sheet| {
                    sheet
                        .cells
                        .iter()
                        .filter_map(|((row, column), cell)| match &cell.value {
                            Some(Value::Error { error }) => Some(json!({
                                "sheet": sheet.name,
                                "cell": cell_name(*row, *column),
                                "error": error,
                                "formula": cell.formula,
                            })),
                            _ => None,
                        })
                })
                .collect();
            json!({ "errors": errors })
        }
    }
}
