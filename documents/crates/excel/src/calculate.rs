use formualizer::{
    common::{ExcelError, ExcelErrorKind, LiteralValue, RangeAddress},
    workbook::{NamedRangeScope, Workbook, WorkbookConfig},
};

use crate::{
    formula::parse_range,
    model::{Model, Value},
};

/// The model with the current result of each formula, as Excel works it out. A formula that the
/// engine cannot work out, such as one with a function that it does not know, keeps its value.
pub(crate) fn calculated(model: &Model) -> Model {
    let mut current = model.clone();
    let formulas = model
        .sheets
        .iter()
        .any(|sheet| sheet.cells.values().any(|cell| cell.formula.is_some()));
    if !formulas {
        return current;
    }
    let Some(book) = workbook(model) else {
        return current;
    };
    for sheet in &mut current.sheets {
        for (&(row, column), cell) in &mut sheet.cells {
            if cell.formula.is_some()
                && let Some(value) = book
                    .get_value(&sheet.name, row + 1, column + 1)
                    .and_then(value_of)
            {
                cell.value = Some(value);
            }
        }
    }
    current
}

fn workbook(model: &Model) -> Option<Workbook> {
    let mut book = Workbook::new_with_config(WorkbookConfig::ephemeral());
    for sheet in &model.sheets {
        book.add_sheet(&sheet.name).ok()?;
    }
    for name in &model.names {
        if let Some(address) = address(&name.text) {
            let scope = if name.sheet.is_some() {
                NamedRangeScope::Sheet
            } else {
                NamedRangeScope::Workbook
            };
            drop(book.define_named_range(&name.name, &address, scope));
        }
    }
    for sheet in &model.sheets {
        for (&(row, column), cell) in &sheet.cells {
            let (row, column) = (row + 1, column + 1);
            match (&cell.formula, &cell.value) {
                (Some(formula), _) => book.set_formula(&sheet.name, row, column, formula).ok()?,
                (None, Some(value)) => book
                    .set_value(&sheet.name, row, column, literal(value))
                    .ok()?,
                (None, None) => {}
            }
        }
    }
    book.evaluate_all().ok()?;
    Some(book)
}

/// `'Sales 2026'!$B$2:$B$9` as a range of the engine.
fn address(text: &str) -> Option<RangeAddress> {
    let (sheet, cells) = text.trim_start_matches('=').rsplit_once('!')?;
    let sheet = sheet
        .strip_prefix('\'')
        .and_then(|quoted| quoted.strip_suffix('\''))
        .map_or_else(|| sheet.to_owned(), |quoted| quoted.replace("''", "'"));
    let [top, left, bottom, right] = parse_range(&cells.replace('$', ""))?;
    RangeAddress::new(sheet, top + 1, left + 1, bottom + 1, right + 1).ok()
}

fn literal(value: &Value) -> LiteralValue {
    match value {
        Value::Number(number) => LiteralValue::Number(*number),
        Value::Bool(on) => LiteralValue::Boolean(*on),
        Value::Text(text) => LiteralValue::Text(text.clone()),
        Value::Error { error } => LiteralValue::Error(ExcelError::from_error_string(error)),
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "Excel keeps each number as a double"
)]
fn value_of(value: LiteralValue) -> Option<Value> {
    Some(match value {
        LiteralValue::Int(number) => Value::Number(number as f64),
        LiteralValue::Number(number) if number.is_finite() => Value::Number(number),
        LiteralValue::Number(_) => Value::Error {
            error: ExcelErrorKind::Num.to_string(),
        },
        LiteralValue::Text(text) => Value::Text(text),
        LiteralValue::Boolean(on) => Value::Bool(on),
        LiteralValue::Empty => Value::Number(0.0),
        LiteralValue::Array(rows) => {
            return rows
                .into_iter()
                .next()?
                .into_iter()
                .next()
                .and_then(value_of);
        }
        LiteralValue::Error(error)
            if matches!(error.kind, ExcelErrorKind::Name | ExcelErrorKind::NImpl) =>
        {
            return None;
        }
        LiteralValue::Error(error) => Value::Error {
            error: error.kind.to_string(),
        },
        LiteralValue::Pending => return None,
        temporal => Value::Number(temporal.as_serial_number()?),
    })
}
