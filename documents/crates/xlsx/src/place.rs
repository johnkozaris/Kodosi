use serde_json::{Value as Json, json};

use crate::{
    formula::{Shift, cell_name},
    model::{Change, Model},
};

fn line(place: &Json, key: &str) -> Option<u32> {
    u32::try_from(place.get(key)?.as_u64()?).ok()
}

pub(crate) fn map(place: &mut Json, change: &Change) {
    match change {
        Change::Group { changes } => {
            for inner in changes {
                map(place, inner);
            }
        }
        Change::Rows { sheet, at, count } => {
            shift(
                place,
                sheet,
                Shift {
                    at: *at,
                    count: *count,
                },
                ["row", "endRow"],
            );
        }
        Change::Columns { sheet, at, count } => {
            shift(
                place,
                sheet,
                Shift {
                    at: *at,
                    count: *count,
                },
                ["column", "endColumn"],
            );
        }
        _ => {}
    }
}

fn shift(place: &mut Json, sheet: &str, shift: Shift, keys: [&str; 2]) {
    if place.get("sheet").and_then(Json::as_str) != Some(sheet) {
        return;
    }
    for key in keys {
        if let Some(line) = line(place, key) {
            place[key] = json!(shift.line(line).unwrap_or(shift.at));
        }
    }
}

pub(crate) fn describe(model: &Model, place: &Json) -> Json {
    let id = place.get("sheet").and_then(Json::as_str);
    let Some(sheet) = model
        .sheets
        .iter()
        .find(|sheet| Some(sheet.id.as_str()) == id)
    else {
        return Json::Null;
    };
    let (Some(row), Some(column)) = (line(place, "row"), line(place, "column")) else {
        return Json::Null;
    };
    let first = cell_name(row, column);
    let last = cell_name(
        line(place, "endRow").unwrap_or(row),
        line(place, "endColumn").unwrap_or(column),
    );
    let cells = if first == last {
        first
    } else {
        format!("{first}:{last}")
    };
    let mut described = json!({ "range": format!("{}!{cells}", sheet_name(&sheet.name)) });
    if place.get("editing").and_then(Json::as_bool) == Some(true) {
        described["editing"] = json!(true);
    }
    described
}

fn sheet_name(name: &str) -> String {
    if name
        .chars()
        .all(|letter| letter.is_alphanumeric() || letter == '_')
    {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}
