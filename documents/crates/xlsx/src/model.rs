use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, json};

use crate::Op;

/// The .xlsx format.
pub struct Xlsx;

/// What a cell holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Number(f64),
    Bool(bool),
    Text(String),
    Error { error: String },
}

/// One cell. A cell that the file holds as a formula also holds its last value.
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

/// A range that some element of a sheet names, which moves with the rows and columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Anchor {
    pub(crate) tag: &'static str,
    pub(crate) attribute: &'static str,
    pub(crate) ranges: String,
    /// Where a click goes, for a hyperlink. The element keeps it in the file.
    pub(crate) link: Option<Link>,
}

/// An address, or a place in the workbook such as `Sheet2!A1`.
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
    /// The attributes of rows and columns that this crate only keeps.
    pub(crate) row_looks: BTreeMap<u32, String>,
    pub(crate) column_looks: BTreeMap<u32, String>,
    pub(crate) anchors: Vec<Anchor>,
    /// Rows or columns moved, so a save writes the whole sheet.
    pub(crate) moved: bool,
}

/// One look that cells can share.
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

/// The live copy of a .xlsx file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    pub(crate) sheets: Vec<Sheet>,
    /// The looks that cells name by index. A file's styles hold the first of them: those that
    /// it had and those that saves added.
    pub(crate) looks: Vec<Look>,
    /// The defined names, in the order of the file.
    pub(crate) names: Vec<DefinedName>,
    /// The highest sheet number so far, so that a new sheet never takes a removed one's.
    pub(crate) last_number: u32,
}

/// A name of the workbook, such as a range that formulas use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefinedName {
    pub(crate) name: String,
    /// The place of the only sheet that knows this name, if one sheet owns it.
    pub(crate) sheet: Option<usize>,
    pub(crate) text: String,
}

/// A change to one cell, by its key. What it does not name stays.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CellChange {
    pub row: u32,
    pub column: u32,
    /// `null` empties the cell. Without it, the value stays.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Option<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<Option<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Look>,
}

/// One change. Cells name a key, so the last change wins. Rows and columns name positions, so
/// a view moves its older cell changes across them.
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
        #[serde(default)]
        widths: BTreeMap<u32, f64>,
        #[serde(default)]
        heights: BTreeMap<u32, f64>,
    },
    Merges {
        sheet: String,
        #[serde(default)]
        add: Vec<[u32; 4]>,
        #[serde(default)]
        remove: Vec<[u32; 4]>,
    },
    Ops {
        ops: Vec<Op>,
    },
    Group {
        changes: Vec<Self>,
    },
}

/// What read and check ask.
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
        /// Such as `A1:C9`.
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

    /// The part of a sheet, which a sheet made here gets at its first save.
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
