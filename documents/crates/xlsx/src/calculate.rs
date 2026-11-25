use std::{
    collections::HashSet,
    sync::mpsc::{self, RecvTimeoutError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use formualizer::{
    DateSystem, ReferenceType,
    common::{ExcelError, ExcelErrorKind, LiteralValue, RangeAddress},
    eval::engine::CancelToken,
    parse::parse,
    workbook::{NamedRangeScope, Workbook, WorkbookConfig},
};

use crate::{
    formula::parse_range,
    model::{Model, Value},
};

/// Time budget for formula evaluation so heavy or hostile workbooks cannot block the document.
pub(crate) const CALCULATION_TIME: Duration = Duration::from_secs(5);

pub(crate) struct Calculation {
    pub(crate) model: Model,
    /// True when every formula evaluated cleanly without falling back to cached values.
    pub(crate) every_formula: bool,
}

/// Evaluates formulas within `time`. Formulas using unknown functions, cycles, or external
/// references keep their cached values; returns `None` if evaluation times out.
pub(crate) fn calculated(model: &Model, time: Duration) -> Option<Calculation> {
    let mut current = model.clone();
    let formulas = model
        .sheets
        .iter()
        .any(|sheet| sheet.cells.values().any(|cell| cell.formula.is_some()));
    if !formulas {
        return Some(Calculation {
            model: current,
            every_formula: true,
        });
    }
    let (book, unreadable) = workbook(model, time)?;
    let dates = date_system(model);
    let mut every_formula = unreadable.is_empty();
    for (index, sheet) in current.sheets.iter_mut().enumerate() {
        for (&(row, column), cell) in &mut sheet.cells {
            if cell.formula.is_none() || unreadable.contains(&(index, row, column)) {
                continue;
            }
            match book
                .get_value(&sheet.name, row + 1, column + 1)
                .and_then(|value| value_of(value, dates))
            {
                Some(value) => cell.value = Some(value),
                None => every_formula = false,
            }
        }
    }
    Some(Calculation {
        model: current,
        every_formula,
    })
}

pub(crate) fn without_results(model: &Model) -> Model {
    let mut model = model.clone();
    for sheet in &mut model.sheets {
        for cell in sheet.cells.values_mut() {
            if cell.formula.is_some() {
                cell.value = None;
            }
        }
    }
    model
}

const fn date_system(model: &Model) -> DateSystem {
    if model.dates_from_1904 {
        DateSystem::Excel1904
    } else {
        DateSystem::Excel1900
    }
}

/// `formualizer` fails the entire workbook if any formula fails to parse or references an external
/// file, so filter those out up front.
fn readable(formula: &str) -> bool {
    parse(format!("={formula}")).is_ok_and(|ast| {
        !ast.get_dependencies()
            .iter()
            .any(|reference| matches!(reference, ReferenceType::External(_)))
    })
}

/// Inserts cells in reverse order so `formualizer` allocates each sheet at its final size once,
/// and defers dependency graph construction until all cells are loaded.
fn workbook(model: &Model, time: Duration) -> Option<(Workbook, HashSet<(usize, u32, u32)>)> {
    let deadline = Deadline::after(time)?;
    let mut config = WorkbookConfig::ephemeral();
    config.eval = config.eval.with_date_system(date_system(model));
    config.eval.defer_graph_building = true;
    let mut book = Workbook::new_with_config(config);
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
    let mut unreadable = HashSet::new();
    for (index, sheet) in model.sheets.iter().enumerate() {
        for (&(row, column), cell) in sheet.cells.iter().rev() {
            if deadline.passed() {
                return None;
            }
            if let Some(formula) = cell.formula.as_deref().filter(|formula| readable(formula)) {
                book.set_formula(&sheet.name, row + 1, column + 1, formula)
                    .ok()?;
                continue;
            }
            if cell.formula.is_some() {
                unreadable.insert((index, row, column));
            }
            if let Some(value) = &cell.value {
                book.set_value(&sheet.name, row + 1, column + 1, literal(value))
                    .ok()?;
            }
        }
    }
    book.evaluate_all_cancellable(deadline.token.clone()).ok()?;
    Some((book, unreadable))
}

/// Cancels `formualizer`'s evaluation token when `time` expires (polled between formula batches).
struct Deadline {
    token: CancelToken,
    at: Instant,
    stop: Option<mpsc::Sender<()>>,
    timer: Option<JoinHandle<()>>,
}

impl Deadline {
    fn after(time: Duration) -> Option<Self> {
        let token = CancelToken::new();
        let cancel = token.clone();
        let (stop, stopped) = mpsc::channel::<()>();
        let timer = thread::Builder::new()
            .spawn(move || {
                if stopped.recv_timeout(time) == Err(RecvTimeoutError::Timeout) {
                    cancel.cancel();
                }
            })
            .ok()?;
        Some(Self {
            token,
            at: Instant::now() + time,
            stop: Some(stop),
            timer: Some(timer),
        })
    }

    fn passed(&self) -> bool {
        self.token.is_cancelled() || Instant::now() >= self.at
    }
}

impl Drop for Deadline {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(timer) = self.timer.take() {
            drop(timer.join());
        }
    }
}

fn address(text: &str) -> Option<RangeAddress> {
    let (sheet, cells) = text.trim_start_matches('=').rsplit_once('!')?;
    let sheet = sheet
        .strip_circumfix('\'', '\'')
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
    reason = "a sheet keeps each number as a double"
)]
fn value_of(value: LiteralValue, dates: DateSystem) -> Option<Value> {
    Some(match value.finite_or_num_error() {
        LiteralValue::Int(number) => Value::Number(number as f64),
        LiteralValue::Number(number) => Value::Number(number),
        LiteralValue::Text(text) => Value::Text(text),
        LiteralValue::Boolean(on) => Value::Bool(on),
        LiteralValue::Empty => Value::Number(0.0),
        LiteralValue::Array(rows) => {
            return rows
                .into_iter()
                .next()?
                .into_iter()
                .next()
                .and_then(|value| value_of(value, dates));
        }
        LiteralValue::Error(error)
            if matches!(
                error.kind,
                ExcelErrorKind::Name | ExcelErrorKind::NImpl | ExcelErrorKind::Circ
            ) =>
        {
            return None;
        }
        LiteralValue::Error(error) => Value::Error {
            error: error.kind.to_string(),
        },
        LiteralValue::Pending => return None,
        temporal => Value::Number(temporal.as_serial_number_for(dates)?),
    })
}
