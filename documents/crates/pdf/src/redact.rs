use lopdf::{
    Document, Object, ObjectId, Stream,
    content::{Content, Operation},
};

use crate::read::number_of;

/// A wide guess of how far a line of text runs, in sizes of its font for each letter, so a guess
/// errs on the side of removing it.
const WIDEST_LETTER: f64 = 1.1;

type Matrix = [f64; 6];

fn multiply(left: Matrix, right: Matrix) -> Matrix {
    [
        left[1].mul_add(right[2], left[0] * right[0]),
        left[1].mul_add(right[3], left[0] * right[1]),
        left[3].mul_add(right[2], left[2] * right[0]),
        left[3].mul_add(right[3], left[2] * right[1]),
        left[5].mul_add(right[2], left[4] * right[0]) + right[4],
        left[5].mul_add(right[3], left[4] * right[1]) + right[5],
    ]
}

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn matrix_of(operands: &[Object]) -> Option<Matrix> {
    let numbers: Vec<f64> = operands.iter().map(number_of).collect::<Option<_>>()?;
    <[f64; 6]>::try_from(numbers).ok()
}

fn apply(matrix: Matrix, x: f64, y: f64) -> (f64, f64) {
    (
        matrix[2].mul_add(y, matrix[0] * x) + matrix[4],
        matrix[3].mul_add(y, matrix[1] * x) + matrix[5],
    )
}

fn hits(bounds: [f64; 4], rects: &[[f64; 4]]) -> bool {
    rects.iter().any(|rect| {
        bounds[0] < rect[2] && bounds[2] > rect[0] && bounds[1] < rect[3] && bounds[3] > rect[1]
    })
}

fn box_of(points: &[(f64, f64)]) -> [f64; 4] {
    let xs = points.iter().map(|point| point.0);
    let ys = points.iter().map(|point| point.1);
    [
        xs.clone().fold(f64::INFINITY, f64::min),
        ys.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
        ys.fold(f64::NEG_INFINITY, f64::max),
    ]
}

/// How many letters a text operation shows.
fn letters(operation: &Operation) -> usize {
    operation
        .operands
        .iter()
        .map(|operand| match operand {
            Object::String(bytes, _) => bytes.len(),
            Object::Array(items) => items
                .iter()
                .map(|item| match item {
                    Object::String(bytes, _) => bytes.len(),
                    _ => 0,
                })
                .sum(),
            _ => 0,
        })
        .sum()
}

/// The small part of the graphics state that says where text and images land.
struct State {
    graphics: Matrix,
    stack: Vec<Matrix>,
    text: Matrix,
    line: Matrix,
    font_size: f64,
    leading: f64,
}

impl State {
    fn move_line(&mut self, x: f64, y: f64) {
        self.line = multiply([1.0, 0.0, 0.0, 1.0, x, y], self.line);
        self.text = self.line;
    }

    fn text_box(&self, operation: &Operation) -> [f64; 4] {
        let size = self.font_size.abs().max(1.0);
        #[expect(clippy::cast_precision_loss, reason = "a guess of the width")]
        let width = letters(operation) as f64 * size * WIDEST_LETTER;
        let matrix = multiply(self.text, self.graphics);
        box_of(&[
            apply(matrix, 0.0, -size * 0.3),
            apply(matrix, width, -size * 0.3),
            apply(matrix, 0.0, size * 1.1),
            apply(matrix, width, size * 1.1),
        ])
    }

    fn image_box(&self) -> [f64; 4] {
        box_of(&[
            apply(self.graphics, 0.0, 0.0),
            apply(self.graphics, 1.0, 0.0),
            apply(self.graphics, 0.0, 1.0),
            apply(self.graphics, 1.0, 1.0),
        ])
    }

    /// Follows one operation, and says whether it draws in a redacted place.
    fn step(&mut self, operation: &Operation, rects: &[[f64; 4]]) -> bool {
        let number = |index: usize| {
            operation
                .operands
                .get(index)
                .and_then(number_of)
                .unwrap_or(0.0)
        };
        match operation.operator.as_str() {
            "q" => self.stack.push(self.graphics),
            "Q" => {
                if let Some(graphics) = self.stack.pop() {
                    self.graphics = graphics;
                }
            }
            "cm" => {
                if let Some(matrix) = matrix_of(&operation.operands) {
                    self.graphics = multiply(matrix, self.graphics);
                }
            }
            "BT" => {
                self.text = IDENTITY;
                self.line = IDENTITY;
            }
            "Tf" => self.font_size = number(1),
            "TL" => self.leading = number(0),
            "Tm" => {
                if let Some(matrix) = matrix_of(&operation.operands) {
                    self.text = matrix;
                    self.line = matrix;
                }
            }
            "Td" => self.move_line(number(0), number(1)),
            "TD" => {
                self.leading = -number(1);
                self.move_line(number(0), number(1));
            }
            "T*" => self.move_line(0.0, -self.leading),
            "Tj" | "TJ" => return hits(self.text_box(operation), rects),
            "'" | "\"" => {
                self.move_line(0.0, -self.leading);
                return hits(self.text_box(operation), rects);
            }
            "Do" => return hits(self.image_box(), rects),
            _ => {}
        }
        false
    }
}

fn black_boxes(rects: &[[f64; 4]]) -> Vec<Operation> {
    let mut operations = vec![
        Operation::new("q", Vec::new()),
        Operation::new("rg", vec![0.into(), 0.into(), 0.into()]),
    ];
    for [left, bottom, right, top] in rects {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "PDF numbers are this exact"
        )]
        let numbers = [
            *left as f32,
            *bottom as f32,
            (right - left) as f32,
            (top - bottom) as f32,
        ];
        operations.push(Operation::new(
            "re",
            numbers.iter().map(|value| Object::Real(*value)).collect(),
        ));
        operations.push(Operation::new("f", Vec::new()));
    }
    operations.push(Operation::new("Q", Vec::new()));
    operations
}

/// Removes what a page draws in these places, and covers them. Text comes out as whole text
/// operations and pictures as whole objects, so a word next to a redacted one can go too, and
/// no redacted word can stay. All old revisions go with the rewrite.
pub(crate) fn redact(
    file: &[u8],
    page: ObjectId,
    rects: &[[f64; 4]],
) -> Result<Vec<u8>, lopdf::Error> {
    let mut document = Document::load_mem(file)?;
    let content = document.get_page_content(page);
    let mut parsed = Content::decode(&content)?;
    let mut state = State {
        graphics: IDENTITY,
        stack: Vec::new(),
        text: IDENTITY,
        line: IDENTITY,
        font_size: 12.0,
        leading: 0.0,
    };
    parsed
        .operations
        .retain(|operation| !state.step(operation, rects));
    parsed.operations.extend(black_boxes(rects));
    let stream = Stream::new(lopdf::Dictionary::new(), parsed.encode()?);
    let added = document.add_object(stream);
    let dictionary = document.get_dictionary_mut(page)?;
    dictionary.set("Contents", Object::Reference(added));
    remove_notes(&mut document, page, rects)?;
    document.prune_objects();
    let mut bytes = Vec::with_capacity(file.len());
    document.save_to(&mut bytes)?;
    Ok(bytes)
}

fn remove_notes(
    document: &mut Document,
    page: ObjectId,
    rects: &[[f64; 4]],
) -> Result<(), lopdf::Error> {
    let Ok(annotations) = document.get_dictionary(page)?.get(b"Annots") else {
        return Ok(());
    };
    let (_, annotations) = document.dereference(annotations)?;
    let kept: Vec<Object> = annotations
        .as_array()?
        .iter()
        .filter(|annotation| {
            let bounds = document
                .dereference(annotation)
                .ok()
                .and_then(|(_, annotation)| annotation.as_dict().ok())
                .and_then(|annotation| annotation.get(b"Rect").ok())
                .and_then(|rect| rect.as_array().ok())
                .and_then(|rect| rect.iter().map(number_of).collect::<Option<Vec<f64>>>())
                .and_then(|rect| <[f64; 4]>::try_from(rect).ok());
            bounds.is_none_or(|bounds| !hits(bounds, rects))
        })
        .cloned()
        .collect();
    document
        .get_dictionary_mut(page)?
        .set("Annots", Object::Array(kept));
    Ok(())
}
