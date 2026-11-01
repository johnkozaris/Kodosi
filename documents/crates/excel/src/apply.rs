use std::collections::BTreeSet;

use document::{Ordered, Refused};

use crate::{
    formula::{Shift, shift_formula},
    model::{CellChange, Change, Look, Model, Sheet},
    ops,
};

const MOST_LINES: i64 = 1_048_576;

fn no_sheet(id: &str) -> Refused {
    Refused::Invalid(format!("no sheet {id}"))
}

fn moved_lines(since: &[Ordered<Change>], sheet: &str) -> bool {
    since.iter().any(|ordered| match &ordered.change {
        Change::Rows { sheet: moved, .. } | Change::Columns { sheet: moved, .. } => moved == sheet,
        _ => false,
    })
}

pub(crate) fn apply(
    model: &mut Model,
    change: Change,
    since: &[Ordered<Change>],
) -> Result<Change, Refused> {
    match &change {
        Change::Cells { sheet, .. }
        | Change::Rows { sheet, .. }
        | Change::Columns { sheet, .. }
            if moved_lines(since, sheet) =>
        {
            return Err(Refused::Behind(
                "Rows or columns moved. Move the change and send it again.".to_owned(),
            ));
        }
        _ => {}
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
            let mut new_looks = Vec::new();
            for cell in cells.iter_mut() {
                if let Some(look) = cell.look.take() {
                    cell.style = Some(Some(look_index(model, look, &mut new_looks)));
                }
            }
            let target = model.sheet_mut(sheet).ok_or_else(|| no_sheet(sheet))?;
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
                return Err(Refused::Invalid(format!("the sheet {name} exists")));
            }
            model.sheets.push(Sheet {
                id: id.clone(),
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
                return Err(Refused::Invalid("the last sheet stays".to_owned()));
            }
            model.sheets.retain(|known| known.id != *sheet);
        }
        Change::Size {
            sheet,
            widths,
            heights,
        } => {
            let target = model.sheet_mut(sheet).ok_or_else(|| no_sheet(sheet))?;
            target
                .widths
                .extend(widths.iter().map(|(at, width)| (*at, *width)));
            target
                .heights
                .extend(heights.iter().map(|(at, height)| (*at, *height)));
        }
        Change::Group { changes } => {
            for change in changes {
                apply_one(model, change)?;
            }
        }
        Change::Ops { .. } => {
            return Err(Refused::Invalid("ops inside ops".to_owned()));
        }
    }
    Ok(())
}

/// The index of a look, which is new when no cell has it yet.
fn look_index(model: &mut Model, look: Look, _new: &mut Vec<Look>) -> u32 {
    let found = model.looks.iter().position(|known| *known == look);
    let index = found.unwrap_or_else(|| {
        model.looks.push(look);
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
        return Err(Refused::Invalid("no such lines".to_owned()));
    }
    let shift = Shift { at, count };
    let name = model.sheet(id).ok_or_else(|| no_sheet(id))?.name.clone();
    for sheet in &mut model.sheets {
        let own = sheet.id == id;
        for cell in sheet.cells.values_mut() {
            if let Some(formula) = &cell.formula {
                cell.formula = Some(shift_formula(
                    formula,
                    own,
                    &name,
                    rows.then_some(shift),
                    (!rows).then_some(shift),
                ));
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
    Ok(())
}

/// What turns one state of the workbook into another, by cell.
pub(crate) fn compare(from: &Model, to: &Model) -> Vec<Change> {
    let mut changes = Vec::new();
    for sheet in &to.sheets {
        let Some(old) = from.sheet(&sheet.id) else {
            changes.push(Change::AddSheet {
                id: sheet.id.clone(),
                name: sheet.name.clone(),
            });
            changes.extend(cells_change(&Sheet::default(), sheet));
            continue;
        };
        if old.name != sheet.name {
            changes.push(Change::RenameSheet {
                sheet: sheet.id.clone(),
                name: sheet.name.clone(),
            });
        }
        changes.extend(cells_change(old, sheet));
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
