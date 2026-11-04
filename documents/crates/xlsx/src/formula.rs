use std::fmt::Write as _;

/// The name of a column, from 0: A, B, ... Z, AA.
pub(crate) fn column_name(mut column: u32) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push(char::from(b'A' + u8::try_from(column % 26).unwrap_or(0)));
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    letters.iter().rev().collect()
}

pub(crate) fn column_number(letters: &str) -> Option<u32> {
    let mut column = 0_u32;
    for letter in letters.bytes() {
        if !letter.is_ascii_alphabetic() {
            return None;
        }
        column = column
            .checked_mul(26)?
            .checked_add(u32::from(letter.to_ascii_uppercase() - b'A') + 1)?;
    }
    column.checked_sub(1)
}

pub(crate) fn cell_name(row: u32, column: u32) -> String {
    format!("{}{}", column_name(column), row + 1)
}

/// `B2` as a row and a column, from 0.
pub(crate) fn parse_cell(name: &str) -> Option<(u32, u32)> {
    let split = name.find(|letter: char| letter.is_ascii_digit())?;
    let (letters, digits) = name.split_at(split);
    let row: u32 = digits.parse().ok()?;
    Some((row.checked_sub(1)?, column_number(letters)?))
}

/// `A1:C9` as its first and last cell.
pub(crate) fn parse_range(name: &str) -> Option<[u32; 4]> {
    let (first, last) = name.split_once(':').unwrap_or((name, name));
    let (top, left) = parse_cell(first)?;
    let (bottom, right) = parse_cell(last)?;
    Some([top, left, bottom, right])
}

/// `A1` for one cell, or `A1:C9`.
pub(crate) fn range_name([top, left, bottom, right]: [u32; 4]) -> String {
    if (top, left) == (bottom, right) {
        return cell_name(top, left);
    }
    format!("{}:{}", cell_name(top, left), cell_name(bottom, right))
}

/// One end of a reference in a formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reference {
    pub row: Option<u32>,
    pub column: Option<u32>,
    pub row_fixed: bool,
    pub column_fixed: bool,
}

impl Reference {
    fn write(self, out: &mut String) {
        if let Some(column) = self.column {
            if self.column_fixed {
                out.push('$');
            }
            out.push_str(&column_name(column));
        }
        if let Some(row) = self.row {
            if self.row_fixed {
                out.push('$');
            }
            let _ = write!(out, "{}", row + 1);
        }
    }
}

fn parse_reference(text: &str) -> Option<Reference> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let column_fixed = bytes.first() == Some(&b'$');
    index += usize::from(column_fixed);
    let letters = bytes[index..]
        .iter()
        .take_while(|byte| byte.is_ascii_alphabetic())
        .count();
    let column = if letters > 0 {
        let column = column_number(&text[index..index + letters])?;
        (column < 18_278).then_some(column)?;
        Some(column)
    } else {
        None
    };
    index += letters;
    let row_fixed = bytes.get(index) == Some(&b'$');
    index += usize::from(row_fixed);
    let digits = &text[index..];
    let row = if digits.is_empty() {
        None
    } else {
        if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        Some(digits.parse::<u32>().ok()?.checked_sub(1)?)
    };
    (column.is_some() || row.is_some()).then_some(Reference {
        row,
        column,
        row_fixed,
        column_fixed,
    })
}

fn is_name_letter(letter: char) -> bool {
    letter.is_alphanumeric() || matches!(letter, '_' | '.' | '$')
}

type ReferenceEnds = (Reference, Option<Reference>);
type MapReference<'a> =
    dyn Fn(Option<&str>, Reference, Option<Reference>) -> Option<ReferenceEnds> + 'a;

/// Rewrites each reference of a formula, and leaves text, names and functions alone.
/// `change` takes the sheet that the reference names and both of its ends, and it can give
/// nothing, which makes the reference `#REF!`.
pub(crate) fn map_references(formula: &str, change: &MapReference<'_>) -> String {
    let letters: Vec<char> = formula.chars().collect();
    let mut out = String::with_capacity(formula.len());
    let mut index = 0;
    while index < letters.len() {
        let letter = letters[index];
        match letter {
            '"' => {
                let start = index;
                index += 1;
                while index < letters.len() {
                    if letters[index] == '"' {
                        if letters.get(index + 1) == Some(&'"') {
                            index += 1;
                        } else {
                            break;
                        }
                    }
                    index += 1;
                }
                index = (index + 1).min(letters.len());
                out.extend(&letters[start..index]);
            }
            '\'' => {
                let start = index;
                index += 1;
                while index < letters.len() {
                    if letters[index] == '\'' {
                        if letters.get(index + 1) == Some(&'\'') {
                            index += 1;
                        } else {
                            break;
                        }
                    }
                    index += 1;
                }
                index = (index + 1).min(letters.len());
                if letters.get(index) == Some(&'!') {
                    let sheet: String = letters[start + 1..index - 1].iter().collect();
                    let sheet = sheet.replace("''", "'");
                    let prefix: String = letters[start..=index].iter().collect();
                    index += 1;
                    index =
                        copy_reference(&letters, index, Some(&sheet), &prefix, change, &mut out);
                } else {
                    out.extend(&letters[start..index]);
                }
            }
            letter if is_name_letter(letter) => {
                let start = index;
                while index < letters.len() && is_name_letter(letters[index]) {
                    index += 1;
                }
                let word: String = letters[start..index].iter().collect();
                if letters.get(index) == Some(&'!') {
                    let prefix = format!("{word}!");
                    index += 1;
                    index = copy_reference(&letters, index, Some(&word), &prefix, change, &mut out);
                } else if letters.get(index) == Some(&'(') {
                    out.push_str(&word);
                } else {
                    index = copy_reference(&letters, start, None, "", change, &mut out);
                }
            }
            letter => {
                out.push(letter);
                index += 1;
            }
        }
    }
    out
}

fn word_at(letters: &[char], start: usize) -> (String, usize) {
    let mut end = start;
    while end < letters.len() && is_name_letter(letters[end]) {
        end += 1;
    }
    (letters[start..end].iter().collect(), end)
}

fn copy_reference(
    letters: &[char],
    start: usize,
    sheet: Option<&str>,
    prefix: &str,
    change: &MapReference<'_>,
    out: &mut String,
) -> usize {
    let (word, end) = word_at(letters, start);
    let Some(first) = parse_reference(&word) else {
        out.push_str(prefix);
        out.push_str(&word);
        return end;
    };
    let mut second = None;
    let mut last = end;
    if letters.get(end) == Some(&':') {
        let (next, after) = word_at(letters, end + 1);
        if let Some(reference) = parse_reference(&next) {
            second = Some(reference);
            last = after;
        }
    }
    let whole_line = first.row.is_none() || first.column.is_none();
    if whole_line && second.is_none() {
        out.push_str(prefix);
        out.push_str(&word);
        return end;
    }
    out.push_str(prefix);
    match change(sheet, first, second) {
        Some((first, second)) => {
            first.write(out);
            if let Some(second) = second {
                out.push(':');
                second.write(out);
            }
        }
        None => out.push_str("#REF!"),
    }
    last
}

/// A line of rows or columns that was inserted or deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shift {
    pub at: u32,
    pub count: i32,
}

impl Shift {
    /// Where a line goes, or nothing when it was deleted.
    pub(crate) fn line(self, line: u32) -> Option<u32> {
        if self.count >= 0 {
            return Some(if line >= self.at {
                line + self.count.unsigned_abs()
            } else {
                line
            });
        }
        let removed = self.count.unsigned_abs();
        if line < self.at {
            Some(line)
        } else if line < self.at + removed {
            None
        } else {
            Some(line - removed)
        }
    }

    /// Where a range goes: it shrinks when part of it was deleted.
    pub(crate) fn span(self, first: u32, last: u32) -> Option<(u32, u32)> {
        if self.count >= 0 {
            return Some((self.line(first)?, self.line(last)?));
        }
        let removed = self.count.unsigned_abs();
        let start = self
            .line(first)
            .or_else(|| (last >= self.at + removed).then_some(self.at))?;
        let end = self
            .line(last)
            .or_else(|| (first < self.at).then(|| self.at.saturating_sub(1)))?;
        Some((start, end))
    }
}

/// Moves the references of a formula across inserted or deleted rows or columns of one sheet.
pub(crate) fn shift_formula(
    formula: &str,
    own_sheet: bool,
    sheet: &str,
    rows: Option<Shift>,
    columns: Option<Shift>,
) -> String {
    map_references(formula, &|named, first, second| {
        let here = named.map_or(own_sheet, |named| named == sheet);
        if !here {
            return Some((first, second));
        }
        let (mut first, mut second) = (first, second);
        if let Some(shift) = rows {
            move_axis(
                shift,
                &mut first,
                &mut second,
                |reference| reference.row,
                |reference, value| reference.row = value,
            )?;
        }
        if let Some(shift) = columns {
            move_axis(
                shift,
                &mut first,
                &mut second,
                |reference| reference.column,
                |reference, value| reference.column = value,
            )?;
        }
        Some((first, second))
    })
}

/// Moves a list of ranges across inserted or deleted rows or columns. A deleted range goes.
pub(crate) fn shift_ranges(ranges: &str, shift: Shift, rows: bool) -> String {
    ranges
        .split_whitespace()
        .filter_map(|range| {
            let [top, left, bottom, right] = parse_range(range)?;
            if rows {
                let (top, bottom) = shift.span(top, bottom)?;
                Some(range_name([top, left, bottom, right]))
            } else {
                let (left, right) = shift.span(left, right)?;
                Some(range_name([top, left, bottom, right]))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn move_axis(
    shift: Shift,
    first: &mut Reference,
    second: &mut Option<Reference>,
    get: impl Fn(&Reference) -> Option<u32>,
    set: impl Fn(&mut Reference, Option<u32>),
) -> Option<()> {
    let Some(start) = get(first) else {
        return Some(());
    };
    match second {
        Some(end) => {
            let last = get(end)?;
            let (start, last) = shift.span(start, last)?;
            set(first, Some(start));
            set(end, Some(last));
        }
        None => set(first, Some(shift.line(start)?)),
    }
    Some(())
}

/// The formula of a cell in a shared group, from the formula of the group's first cell.
pub(crate) fn translate_shared(formula: &str, rows: i64, columns: i64) -> String {
    map_references(formula, &|_, first, second| {
        let moved = |mut reference: Reference| {
            if !reference.row_fixed
                && let Some(row) = reference.row
            {
                reference.row = Some(u32::try_from(i64::from(row) + rows).ok()?);
            }
            if !reference.column_fixed
                && let Some(column) = reference.column
            {
                reference.column = Some(u32::try_from(i64::from(column) + columns).ok()?);
            }
            Some(reference)
        };
        let first = moved(first)?;
        match second {
            Some(second) => Some((first, Some(moved(second)?))),
            None => Some((first, None)),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(formula: &str, at: u32, count: i32) -> String {
        shift_formula(formula, true, "Sheet1", Some(Shift { at, count }), None)
    }

    #[test]
    fn names_of_columns_and_cells_go_both_ways() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(702), "AAA");
        assert_eq!(parse_cell("AA10"), Some((9, 26)));
        assert_eq!(cell_name(9, 26), "AA10");
        assert_eq!(parse_range("A1:C9"), Some([0, 0, 8, 2]));
        assert_eq!(parse_cell("A0"), None);
    }

    #[test]
    fn inserted_rows_move_references_below_them() {
        assert_eq!(rows("A1+A5", 2, 3), "A1+A8");
        assert_eq!(rows("SUM($B$2:B9)", 3, 1), "SUM($B$2:B10)");
    }

    #[test]
    fn deleted_rows_shrink_ranges_and_break_single_cells() {
        assert_eq!(rows("SUM(A1:A9)", 2, -2), "SUM(A1:A7)");
        assert_eq!(rows("A3*2", 2, -1), "#REF!*2");
        assert_eq!(rows("SUM(A3:A4)", 2, -2), "SUM(#REF!)");
    }

    #[test]
    fn text_functions_and_names_stay() {
        assert_eq!(
            rows(r#"IF(LOG10(A5)>1,"A5",Total)"#, 0, 1),
            r#"IF(LOG10(A6)>1,"A5",Total)"#
        );
    }

    #[test]
    fn references_to_other_sheets_stay_and_named_ones_move() {
        assert_eq!(rows("Other!A5+Sheet1!A5", 0, 1), "Other!A5+Sheet1!A6");
        assert_eq!(rows("'Sheet1'!A5", 0, 1), "'Sheet1'!A6");
        assert_eq!(
            shift_formula(
                "A5+Sheet1!A5",
                false,
                "Sheet1",
                Some(Shift { at: 0, count: 1 }),
                None
            ),
            "A5+Sheet1!A6"
        );
    }

    #[test]
    fn whole_columns_stay_when_rows_move() {
        assert_eq!(rows("SUM(A:A)", 0, 1), "SUM(A:A)");
    }

    #[test]
    fn a_shared_formula_moves_its_relative_references() {
        assert_eq!(translate_shared("A1*$B$1+B$2", 2, 1), "B3*$B$1+C$2");
    }
}
