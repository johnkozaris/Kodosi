use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use serde_json::{Map, json};

use crate::{Op, formula::Shift};

pub struct Xlsx;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Number(f64),
    Bool(bool),
    Text(String),
    Error { error: String },
}

/// A cell that the file holds as a formula also holds its last value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<u32>,
}

impl Cell {
    pub(crate) fn is_empty(&self) -> bool {
        self.value.is_none() && self.formula.is_none() && self.style.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Anchor {
    pub(crate) tag: &'static str,
    pub(crate) attribute: &'static str,
    pub(crate) ranges: String,
    pub(crate) link: Option<Link>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Link {
    Href(String),
    Location(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Sheet {
    pub(crate) id: String,
    /// The number that the file knows this sheet by.
    pub(crate) number: u32,
    pub(crate) name: String,
    pub(crate) part: Option<String>,
    pub(crate) cells: BTreeMap<(u32, u32), Cell>,
    pub(crate) merges: Vec<[u32; 4]>,
    pub(crate) widths: BTreeMap<u32, f64>,
    pub(crate) heights: BTreeMap<u32, f64>,
    pub(crate) hidden_rows: BTreeSet<u32>,
    pub(crate) hidden_columns: BTreeSet<u32>,
    pub(crate) frozen: Frozen,
    /// The attributes of rows and columns that this crate only keeps.
    pub(crate) row_looks: BTreeMap<u32, String>,
    pub(crate) column_looks: BTreeMap<u32, String>,
    pub(crate) anchors: Vec<Anchor>,
    /// Rows or columns moved, so a save writes the whole sheet.
    pub(crate) moved: bool,
    /// A chart sheet, like any sheet without sheet data, has no cells: a save keeps it as it is,
    /// and a change to its cells, rows or columns is refused.
    pub(crate) chart: bool,
}

/// The rows and the columns that stay in sight while the rest of a sheet scrolls: `rows` rows from
/// `top` and `columns` columns from `left`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub(crate) struct Frozen {
    pub(crate) top: u32,
    pub(crate) left: u32,
    pub(crate) rows: u32,
    pub(crate) columns: u32,
}

impl Frozen {
    /// A side with nothing frozen starts at 0, so that equal panes compare equal.
    pub(crate) fn new(top: u32, left: u32, rows: u32, columns: u32) -> Self {
        Self {
            top: if rows == 0 { 0 } else { top },
            left: if columns == 0 { 0 } else { left },
            rows,
            columns,
        }
    }

    pub(crate) fn is_frozen(self) -> bool {
        self.rows > 0 || self.columns > 0
    }

    /// Lines that come in before the first line that scrolls join the frozen ones, and lines that
    /// go before it leave them, though one stays. Univer moves the panes this way for a person's
    /// own change, so every copy moves them alike.
    pub(crate) fn moved(self, shift: Shift, rows: bool) -> Self {
        let split = |first: u32, count: u32| -> (u32, u32) {
            let end = first + count;
            if count == 0 || shift.at >= end {
                return (first, count);
            }
            let lines = shift.count.unsigned_abs();
            if shift.count > 0 {
                return (first, count.saturating_add(lines));
            }
            let gone = end.min(shift.at.saturating_add(lines)) - shift.at;
            let end = (end - gone).max(1);
            let count = count.saturating_sub(gone).max(1);
            (end - count, count)
        };
        if rows {
            let (top, rows) = split(self.top, self.rows);
            Self { top, rows, ..self }
        } else {
            let (left, columns) = split(self.left, self.columns);
            Self {
                left,
                columns,
                ..self
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is one switch of a cell's look"
)]
pub struct Look {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strike: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wrap: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    pub(crate) sheets: Vec<Sheet>,
    /// The looks that cells name by index. A file's styles hold the first of them: those that
    /// it had and those that saves added.
    pub(crate) looks: Vec<Look>,
    pub(crate) names: Vec<DefinedName>,
    /// The highest sheet number so far, so that a new sheet never takes a removed one's.
    pub(crate) last_number: u32,
    /// Whether the file counts days from 1904, as Excel for Mac once did, rather than from 1900.
    pub(crate) dates_from_1904: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefinedName {
    pub(crate) name: String,
    /// The place of the only sheet that knows this name, if one sheet owns it.
    pub(crate) sheet: Option<usize>,
    pub(crate) text: String,
}

/// serde reads `null` as a field that is not there, which would keep what `null` empties.
#[expect(
    clippy::option_option,
    reason = "a field that is not there keeps, and `null` empties"
)]
fn emptied<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    value: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(value).map(Some)
}

/// What a change does not name stays.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CellChange {
    pub row: u32,
    pub column: u32,
    /// `null` empties the cell. Without it, the value stays.
    #[serde(
        default,
        deserialize_with = "emptied",
        skip_serializing_if = "Option::is_none"
    )]
    pub value: Option<Option<Value>>,
    #[serde(
        default,
        deserialize_with = "emptied",
        skip_serializing_if = "Option::is_none"
    )]
    pub formula: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "emptied",
        skip_serializing_if = "Option::is_none"
    )]
    pub style: Option<Option<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Look>,
}

/// JSON names a line by its number as text, which serde does not read as a number in a tagged
/// change.
fn line_keys<'de, D: Deserializer<'de>, V: Deserialize<'de>>(
    lines: D,
) -> Result<BTreeMap<u32, V>, D::Error> {
    BTreeMap::<String, V>::deserialize(lines)?
        .into_iter()
        .map(|(line, value)| Ok((line.parse().map_err(D::Error::custom)?, value)))
        .collect()
}

/// Cells name a key, so the last change wins. Rows and columns name positions, so a view moves
/// its older cell changes across them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Change {
    Cells {
        sheet: String,
        cells: Vec<CellChange>,
    },
    Rows {
        sheet: String,
        at: u32,
        /// Positive inserts, negative deletes.
        count: i32,
    },
    Columns {
        sheet: String,
        at: u32,
        count: i32,
    },
    AddSheet {
        id: String,
        name: String,
    },
    RenameSheet {
        sheet: String,
        name: String,
    },
    RemoveSheet {
        sheet: String,
    },
    Size {
        sheet: String,
        #[serde(default, deserialize_with = "line_keys")]
        widths: BTreeMap<u32, f64>,
        /// `null` makes a row fit what it holds.
        #[serde(default, deserialize_with = "line_keys")]
        heights: BTreeMap<u32, Option<f64>>,
    },
    Merges {
        sheet: String,
        #[serde(default)]
        add: Vec<[u32; 4]>,
        #[serde(default)]
        remove: Vec<[u32; 4]>,
    },
    /// `true` hides a line and `false` shows it.
    Hidden {
        sheet: String,
        #[serde(default, deserialize_with = "line_keys")]
        rows: BTreeMap<u32, bool>,
        #[serde(default, deserialize_with = "line_keys")]
        columns: BTreeMap<u32, bool>,
    },
    /// No rows and no columns unfreeze the sheet.
    Freeze {
        sheet: String,
        top: u32,
        left: u32,
        rows: u32,
        columns: u32,
    },
    Ops {
        ops: Vec<Op>,
    },
    Group {
        changes: Vec<Self>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Query {
    Read {
        #[serde(default)]
        sheet: Option<String>,
        #[serde(default)]
        range: Option<String>,
    },
    Check,
}

impl Model {
    pub(crate) fn sheet(&self, id: &str) -> Option<&Sheet> {
        self.sheets.iter().find(|sheet| sheet.id == id)
    }

    pub(crate) fn sheet_mut(&mut self, id: &str) -> Option<&mut Sheet> {
        self.sheets.iter_mut().find(|sheet| sheet.id == id)
    }

    pub(crate) fn snapshot(&self) -> serde_json::Value {
        let sheets: Vec<serde_json::Value> = self.sheets.iter().map(Sheet::snapshot).collect();
        json!({ "sheets": sheets, "looks": self.looks })
    }
}

impl Sheet {
    fn snapshot(&self) -> serde_json::Value {
        let mut rows: BTreeMap<u32, Map<String, serde_json::Value>> = BTreeMap::new();
        for ((row, column), cell) in &self.cells {
            rows.entry(*row)
                .or_default()
                .insert(column.to_string(), json!(cell));
        }
        let (last_row, last_column) = self.size();
        json!({
            "id": self.id,
            "name": self.name,
            "rows": last_row,
            "columns": last_column,
            "cells": rows,
            "merges": self.merges,
            "widths": self.widths,
            "heights": self.heights,
            "hiddenRows": self.hidden_rows,
            "hiddenColumns": self.hidden_columns,
            "frozen": self.frozen,
            "links": self
                .anchors
                .iter()
                .filter(|anchor| !anchor.ranges.is_empty())
                .filter_map(|anchor| {
                    let link = anchor.link.as_ref()?;
                    Some(json!({ "ranges": anchor.ranges, "to": link }))
                })
                .collect::<Vec<_>>(),
        })
    }

    pub(crate) fn part_name(&self) -> String {
        self.part
            .clone()
            .unwrap_or_else(|| format!("xl/worksheets/sheetN{}.xml", self.number))
    }

    pub(crate) fn size(&self) -> (u32, u32) {
        let row = self
            .cells
            .keys()
            .map(|(row, _)| *row + 1)
            .max()
            .unwrap_or(0);
        let column = self
            .cells
            .keys()
            .map(|(_, column)| *column + 1)
            .max()
            .unwrap_or(0);
        (row, column)
    }
}
