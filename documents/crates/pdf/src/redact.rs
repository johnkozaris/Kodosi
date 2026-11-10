use lopdf::{
    Dictionary, Document, Object, ObjectId, Stream,
    content::{Content, Operation},
};

use crate::read::{inherited, number_of};

/// A wide guess of how far a line of text runs, in sizes of its font for each letter, so a guess
/// errs on the side of removing it.
const WIDEST_LETTER: f64 = 1.1;
/// How many forms deep a redaction follows a form that draws another one.
const DEEPEST_FORM: usize = 16;

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

/// Where a box of user space lands on the page under a matrix.
fn landing([left, bottom, right, top]: [f64; 4], matrix: Matrix) -> [f64; 4] {
    box_of(&[
        apply(matrix, left, bottom),
        apply(matrix, right, bottom),
        apply(matrix, left, top),
        apply(matrix, right, top),
    ])
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
    fn new(graphics: Matrix, font_size: f64, leading: f64) -> Self {
        Self {
            graphics,
            stack: Vec::new(),
            text: IDENTITY,
            line: IDENTITY,
            font_size,
            leading,
        }
    }

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
        landing([0.0, 0.0, 1.0, 1.0], self.graphics)
    }

    /// Follows one operation, and says whether it draws in a redacted place. What `Do` draws
    /// takes the resources, so a redaction follows it.
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
            "BI" => return hits(self.image_box(), rects),
            _ => {}
        }
        false
    }
}

/// What a redaction cannot look into, which it refuses rather than leave words there.
fn unsure(name: &[u8]) -> String {
    format!(
        "The page draws {}, which a redaction cannot look into, so it redacts nothing.",
        String::from_utf8_lossy(name)
    )
}

fn failed(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn dictionary_of(document: &Document, object: &Object) -> Option<Dictionary> {
    let (_, object) = document.dereference(object).ok()?;
    object.as_dict().ok().cloned()
}

fn numbers_of(document: &Document, object: &Object) -> Option<Vec<f64>> {
    let (_, object) = document.dereference(object).ok()?;
    object.as_array().ok()?.iter().map(number_of).collect()
}

/// The object that a name of some resources draws.
fn xobject(document: &Document, resources: &Dictionary, name: &[u8]) -> Option<ObjectId> {
    let (_, objects) = document.dereference(resources.get(b"XObject").ok()?).ok()?;
    objects.as_dict().ok()?.get(name).ok()?.as_reference().ok()
}

/// One redaction of the document, which follows forms into what they draw.
struct Redaction<'r> {
    document: &'r mut Document,
    rects: &'r [[f64; 4]],
}

impl Redaction<'_> {
    /// The operations without the ones that draw in the redacted places.
    fn operations(
        &mut self,
        operations: Vec<Operation>,
        resources: &Dictionary,
        mut state: State,
        depth: usize,
    ) -> Result<Vec<Operation>, String> {
        let mut kept = Vec::with_capacity(operations.len());
        for operation in operations {
            let drawn_there = if operation.operator == "Do" {
                self.object(&operation, resources, &state, depth)?
            } else {
                state.step(&operation, self.rects)
            };
            if !drawn_there {
                kept.push(operation);
            }
        }
        Ok(kept)
    }

    /// Whether `Do` draws a picture in a redacted place. A form loses what it draws there.
    fn object(
        &mut self,
        operation: &Operation,
        resources: &Dictionary,
        state: &State,
        depth: usize,
    ) -> Result<bool, String> {
        let name = operation
            .operands
            .first()
            .and_then(|name| name.as_name().ok())
            .unwrap_or_default();
        let id = xobject(self.document, resources, name).ok_or_else(|| unsure(name))?;
        let kind = self
            .document
            .get_object(id)
            .and_then(Object::as_stream)
            .and_then(|stream| stream.dict.get(b"Subtype"))
            .and_then(Object::as_name)
            .map_err(|_| unsure(name))?;
        match kind {
            b"Image" => Ok(hits(state.image_box(), self.rects)),
            b"Form" if depth < DEEPEST_FORM => {
                self.form(id, resources, state, depth + 1)?;
                Ok(false)
            }
            _ => Err(unsure(name)),
        }
    }

    /// Takes out of a form what it draws in the redacted places, where the state puts it. The
    /// form loses it wherever it is drawn, so that no redacted word stays in the file.
    fn form(
        &mut self,
        id: ObjectId,
        outer: &Dictionary,
        state: &State,
        depth: usize,
    ) -> Result<(), String> {
        let stream = self
            .document
            .get_object(id)
            .and_then(Object::as_stream)
            .map_err(failed)?;
        let matrix = stream
            .dict
            .get(b"Matrix")
            .ok()
            .and_then(|matrix| numbers_of(self.document, matrix))
            .and_then(|numbers| <[f64; 6]>::try_from(numbers).ok())
            .unwrap_or(IDENTITY);
        let graphics = multiply(matrix, state.graphics);
        let bounds = stream
            .dict
            .get(b"BBox")
            .ok()
            .and_then(|bounds| numbers_of(self.document, bounds))
            .and_then(|numbers| <[f64; 4]>::try_from(numbers).ok());
        if bounds.is_some_and(|bounds| !hits(landing(bounds, graphics), self.rects)) {
            return Ok(());
        }
        let resources = match stream.dict.get(b"Resources") {
            Ok(resources) => dictionary_of(self.document, resources)
                .ok_or_else(|| "A form has broken resources.".to_owned())?,
            Err(_) => outer.clone(),
        };
        let content = stream.decompressed_content().map_err(failed)?;
        let operations = Content::decode(&content).map_err(failed)?.operations;
        let start = State::new(graphics, state.font_size, state.leading);
        let kept = self.operations(operations, &resources, start, depth)?;
        let content = Content { operations: kept }.encode().map_err(failed)?;
        self.document
            .get_object_mut(id)
            .and_then(Object::as_stream_mut)
            .map_err(failed)?
            .set_plain_content(content);
        Ok(())
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
/// no redacted word can stay. A form that draws there loses what it draws there, and a page
/// that draws something that a redaction cannot look into is not redacted. All old revisions go
/// with the rewrite.
pub(crate) fn redact(file: &[u8], page: ObjectId, rects: &[[f64; 4]]) -> Result<Vec<u8>, String> {
    let mut document = Document::load_mem(file).map_err(failed)?;
    let resources = document
        .get_dictionary(page)
        .ok()
        .and_then(|dictionary| inherited(&document, dictionary, b"Resources"))
        .and_then(|resources| dictionary_of(&document, resources))
        .unwrap_or_default();
    let content = document.get_page_content(page);
    let operations = Content::decode(&content).map_err(failed)?.operations;
    let mut redaction = Redaction {
        document: &mut document,
        rects,
    };
    let start = State::new(IDENTITY, 12.0, 0.0);
    let mut operations = redaction.operations(operations, &resources, start, 0)?;
    operations.extend(black_boxes(rects));
    let content = Content { operations }.encode().map_err(failed)?;
    let added = document.add_object(Stream::new(Dictionary::new(), content));
    let dictionary = document.get_dictionary_mut(page).map_err(failed)?;
    dictionary.set("Contents", Object::Reference(added));
    remove_notes(&mut document, page, rects).map_err(failed)?;
    document.prune_objects();
    let mut bytes = Vec::with_capacity(file.len());
    document.save_to(&mut bytes).map_err(failed)?;
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
