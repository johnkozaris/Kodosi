use std::collections::{BTreeMap, BTreeSet};

use document::{Ordered, Refused};

use crate::{
    formula::{Shift, shift_formula, shift_ranges},
    model::{CellChange, Change, Frozen, Look, Model, Sheet},
    ops,
};

const MOST_LINES: i64 = 1_048_576;
const MOST_ROWS: u32 = 1_048_576;
const MOST_COLUMNS: u32 = 16_384;

fn no_sheet(id: &str) -> Refused {
    Refused::Invalid(format!("There is no sheet {id}."))
}

fn not_there() -> Refused {
    Refused::Invalid("Those rows or columns are not there.".to_owned())
}

fn within(lines: impl IntoIterator<Item = u32>, rows: bool) -> Result<(), Refused> {
    let most = if rows { MOST_ROWS } else { MOST_COLUMNS };
    if lines.into_iter().any(|line| line >= most) {
        return Err(not_there());
    }
    Ok(())
}

fn sheet_with_cells<'m>(model: &'m mut Model, id: &str) -> Result<&'m mut Sheet, Refused> {
    let sheet = model.sheet_mut(id).ok_or_else(|| no_sheet(id))?;
    if sheet.chart {
        return Err(Refused::Invalid(format!(
            "The sheet {} is a chart and has no cells.",
            sheet.name
        )));
    }
    Ok(sheet)
}

fn moves_lines(change: &Change, sheet: &str) -> bool {
    match change {
        Change::Rows { sheet: moved, .. } | Change::Columns { sheet: moved, .. } => moved == sheet,
        Change::Group { changes } => changes.iter().any(|change| moves_lines(change, sheet)),
        _ => false,
    }
}

fn moved_lines(since: &[Ordered<Change>], sheet: &str) -> bool {
    since
        .iter()
        .any(|ordered| moves_lines(&ordered.change, sheet))
}

fn is_behind(change: &Change, since: &[Ordered<Change>]) -> bool {
    match change {
        Change::Cells { sheet, .. }
        | Change::Rows { sheet, .. }
        | Change::Columns { sheet, .. }
        | Change::Merges { sheet, .. }
        | Change::Size { sheet, .. }
        | Change::Hidden { sheet, .. }
        | Change::Freeze { sheet, .. } => moved_lines(since, sheet),
        Change::Group { changes } => changes.iter().any(|change| is_behind(change, since)),
        _ => false,
    }
}

pub(crate) fn apply(
    model: &mut Model,
    change: Change,
    since: &[Ordered<Change>],
) -> Result<Change, Refused> {
    if is_behind(&change, since) {
        return Err(Refused::Behind(
            "Rows or columns moved. Move the change and send it again.".to_owned(),
        ));
    }
    let mut change = change;
    if let Change::Ops { ops } = &change {
        ops::check_conflicts(ops, model, since)?;
        change = Change::Group {
            changes: ops::to_changes(model, ops)?,
        };
    }
    let mut copy = model.clone();
    apply_one(&mut copy, &mut change)?;
    *model = copy;
    Ok(change)
}

fn apply_one(model: &mut Model, change: &mut Change) -> Result<(), Refused> {
    match change {
        Change::Cells { sheet, cells } => {
            for cell in cells.iter_mut() {
                if let Some(look) = &cell.look {
                    cell.style = Some(Some(look_index(model, look)));
                }
            }
            let target = sheet_with_cells(model, sheet)?;
            for cell in cells.iter() {
                set_cell(target, cell);
            }
        }
        Change::Rows { sheet, at, count } => move_lines(model, sheet, *at, *count, true)?,
        Change::Columns { sheet, at, count } => move_lines(model, sheet, *at, *count, false)?,
        Change::AddSheet { id, name } => {
            if model
                .sheets
                .iter()
                .any(|sheet| sheet.id == *id || sheet.name == *name)
            {
                return Err(Refused::Invalid(format!(
                    "The sheet {name} is there already."
                )));
            }
            model.last_number += 1;
            model.sheets.push(Sheet {
                id: id.clone(),
                number: model.last_number,
                name: name.clone(),
                ..Sheet::default()
            });
        }
        Change::RenameSheet { sheet, name } => {
            model
                .sheet_mut(sheet)
                .ok_or_else(|| no_sheet(sheet))?
                .name
                .clone_from(name);
        }
        Change::RemoveSheet { sheet } => {
            if model.sheets.len() == 1 {
                return Err(Refused::Invalid(
                    "A workbook keeps its last sheet.".to_owned(),
                ));
            }
            model.sheets.retain(|known| known.id != *sheet);
        }
        Change::Size {
            sheet,
            widths,
            heights,
        } => {
            let target = sheet_with_cells(model, sheet)?;
            target
                .widths
                .extend(widths.iter().map(|(at, width)| (*at, *width)));
            for (at, height) in heights.iter() {
                match height {
                    Some(height) => target.heights.insert(*at, *height),
                    None => target.heights.remove(at),
                };
            }
        }
        Change::Merges { sheet, add, remove } => {
            let target = sheet_with_cells(model, sheet)?;
            target.merges.retain(|merge| !remove.contains(merge));
            for merge in add.iter() {
                if !target.merges.contains(merge) {
                    target.merges.push(*merge);
                }
            }
        }
        Change::Hidden {
            sheet,
            rows,
            columns,
        } => hide(sheet_with_cells(model, sheet)?, rows, columns)?,
        Change::Freeze {
            sheet,
            top,
            left,
            rows,
            columns,
        } => {
            let frozen = Frozen::new(*top, *left, *rows, *columns);
            within([frozen.top.saturating_add(frozen.rows)], true)?;
            within([frozen.left.saturating_add(frozen.columns)], false)?;
            sheet_with_cells(model, sheet)?.frozen = frozen;
        }
        Change::Group { changes } => {
            for change in changes {
                apply_one(model, change)?;
            }
        }
        Change::Ops { .. } => {
            return Err(Refused::Invalid("Ops cannot hold ops.".to_owned()));
        }
    }
    Ok(())
}

fn hide(
    sheet: &mut Sheet,
    rows: &BTreeMap<u32, bool>,
    columns: &BTreeMap<u32, bool>,
) -> Result<(), Refused> {
    within(rows.keys().copied(), true)?;
    within(columns.keys().copied(), false)?;
    for (lines, hidden) in [
        (&mut sheet.hidden_rows, rows),
        (&mut sheet.hidden_columns, columns),
    ] {
        for (line, hide) in hidden {
            if *hide {
                lines.insert(*line);
            } else {
                lines.remove(line);
            }
        }
    }
    Ok(())
}

/// Interns `look` into `model.looks`. Callers keep `cell.look` on the broadcast change so views
/// that have not seen the new style can render it.
fn look_index(model: &mut Model, look: &Look) -> u32 {
    let found = model.looks.iter().position(|known| known == look);
    let index = found.unwrap_or_else(|| {
        model.looks.push(look.clone());
        model.looks.len() - 1
    });
    u32::try_from(index).unwrap_or(0)
}

fn set_cell(sheet: &mut Sheet, change: &CellChange) {
    let key = (change.row, change.column);
    let mut cell = sheet.cells.remove(&key).unwrap_or_default();
    if let Some(value) = &change.value {
        cell.value.clone_from(value);
    }
    if let Some(formula) = &change.formula {
        cell.formula.clone_from(formula);
    }
    if let Some(style) = change.style {
        cell.style = style;
    }
    if !cell.is_empty() {
        sheet.cells.insert(key, cell);
    }
}

fn move_lines(model: &mut Model, id: &str, at: u32, count: i32, rows: bool) -> Result<(), Refused> {
    if count == 0 || i64::from(at) + i64::from(count.abs()) > MOST_LINES {
        return Err(not_there());
    }
    let shift = Shift { at, count };
    let name = sheet_with_cells(model, id)?.name.clone();
    let (row_shift, column_shift) = (rows.then_some(shift), (!rows).then_some(shift));
    for defined in &mut model.names {
        defined.text = shift_formula(&defined.text, false, &name, row_shift, column_shift);
    }
    for sheet in &mut model.sheets {
        let own = sheet.id == id;
        for cell in sheet.cells.values_mut() {
            if let Some(formula) = &cell.formula {
                cell.formula = Some(shift_formula(formula, own, &name, row_shift, column_shift));
            }
        }
    }
    let sheet = model.sheet_mut(id).ok_or_else(|| no_sheet(id))?;
    sheet.moved = true;
    let cells = std::mem::take(&mut sheet.cells);
    sheet.cells = cells
        .into_iter()
        .filter_map(|((row, column), cell)| {
            let key = if rows {
                (shift.line(row)?, column)
            } else {
                (row, shift.line(column)?)
            };
            Some((key, cell))
        })
        .collect();
    sheet.merges = std::mem::take(&mut sheet.merges)
        .into_iter()
        .filter_map(|[top, left, bottom, right]| {
            if rows {
                Some([shift.line(top)?, left, shift.line(bottom)?, right])
            } else {
                Some([top, shift.line(left)?, bottom, shift.line(right)?])
            }
        })
        .collect();
    let (sizes, looks, hidden) = if rows {
        (
            &mut sheet.heights,
            &mut sheet.row_looks,
            &mut sheet.hidden_rows,
        )
    } else {
        (
            &mut sheet.widths,
            &mut sheet.column_looks,
            &mut sheet.hidden_columns,
        )
    };
    *sizes = moved_keys(sizes, shift);
    *looks = moved_keys(looks, shift);
    *hidden = hidden.iter().filter_map(|line| shift.line(*line)).collect();
    sheet.frozen = sheet.frozen.moved(shift, rows);
    for anchor in &mut sheet.anchors {
        anchor.ranges = shift_ranges(&anchor.ranges, shift, rows);
    }
    Ok(())
}

fn moved_keys<Value: Clone>(lines: &BTreeMap<u32, Value>, shift: Shift) -> BTreeMap<u32, Value> {
    lines
        .iter()
        .filter_map(|(line, value)| Some((shift.line(*line)?, value.clone())))
        .collect()
}

pub(crate) fn compare(from: &Model, to: &Model) -> Vec<Change> {
    let mut changes = Vec::new();
    for sheet in &to.sheets {
        let Some(old) = from.sheet(&sheet.id) else {
            changes.push(Change::AddSheet {
                id: sheet.id.clone(),
                name: sheet.name.clone(),
            });
            changes.extend(cells_change(&Sheet::default(), sheet));
            changes.extend(layout_changes(&Sheet::default(), sheet));
            continue;
        };
        if old.name != sheet.name {
            changes.push(Change::RenameSheet {
                sheet: sheet.id.clone(),
                name: sheet.name.clone(),
            });
        }
        changes.extend(cells_change(old, sheet));
        changes.extend(layout_changes(old, sheet));
    }
    for sheet in &from.sheets {
        if to.sheet(&sheet.id).is_none() {
            changes.push(Change::RemoveSheet {
                sheet: sheet.id.clone(),
            });
        }
    }
    changes
}

fn sizes_change(old: &BTreeMap<u32, f64>, new: &BTreeMap<u32, f64>) -> BTreeMap<u32, Option<f64>> {
    old.keys()
        .chain(new.keys())
        .filter(|line| old.get(line) != new.get(line))
        .map(|line| (*line, new.get(line).copied()))
        .collect()
}

fn layout_changes(old: &Sheet, new: &Sheet) -> Vec<Change> {
    let mut changes = Vec::new();
    let widths: BTreeMap<u32, f64> = sizes_change(&old.widths, &new.widths)
        .into_iter()
        .filter_map(|(line, width)| Some((line, width?)))
        .collect();
    let heights = sizes_change(&old.heights, &new.heights);
    if !widths.is_empty() || !heights.is_empty() {
        changes.push(Change::Size {
            sheet: new.id.clone(),
            widths,
            heights,
        });
    }
    let others = |from: &[[u32; 4]], to: &[[u32; 4]]| -> Vec<[u32; 4]> {
        from.iter()
            .filter(|merge| !to.contains(merge))
            .copied()
            .collect()
    };
    let add = others(&new.merges, &old.merges);
    let remove = others(&old.merges, &new.merges);
    if !add.is_empty() || !remove.is_empty() {
        changes.push(Change::Merges {
            sheet: new.id.clone(),
            add,
            remove,
        });
    }
    let rows = hidden_change(&old.hidden_rows, &new.hidden_rows);
    let columns = hidden_change(&old.hidden_columns, &new.hidden_columns);
    if !rows.is_empty() || !columns.is_empty() {
        changes.push(Change::Hidden {
            sheet: new.id.clone(),
            rows,
            columns,
        });
    }
    if old.frozen != new.frozen {
        let Frozen {
            top,
            left,
            rows,
            columns,
        } = new.frozen;
        changes.push(Change::Freeze {
            sheet: new.id.clone(),
            top,
            left,
            rows,
            columns,
        });
    }
    changes
}

fn hidden_change(old: &BTreeSet<u32>, new: &BTreeSet<u32>) -> BTreeMap<u32, bool> {
    old.symmetric_difference(new)
        .map(|line| (*line, new.contains(line)))
        .collect()
}

fn cells_change(old: &Sheet, new: &Sheet) -> Option<Change> {
    let keys: BTreeSet<(u32, u32)> = old.cells.keys().chain(new.cells.keys()).copied().collect();
    let mut cells = Vec::new();
    for key in keys {
        let (before, after) = (old.cells.get(&key), new.cells.get(&key));
        if before == after {
            continue;
        }
        let after = after.cloned().unwrap_or_default();
        cells.push(CellChange {
            row: key.0,
            column: key.1,
            value: Some(after.value),
            formula: Some(after.formula),
            style: Some(after.style),
            look: None,
        });
    }
    (!cells.is_empty()).then(|| Change::Cells {
        sheet: new.id.clone(),
        cells,
    })
}
