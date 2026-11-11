use std::collections::HashMap;

use roxmltree::{Document, Node as Xml};
use serde_json::{Map, Value, json};

use crate::read::{is_word, word_attribute};

fn child<'a, 'input>(node: Xml<'a, 'input>, name: &str) -> Option<Xml<'a, 'input>> {
    node.children().find(|child| is_word(*child, name))
}

fn value<'a>(node: Xml<'a, '_>, name: &str) -> Option<&'a str> {
    child(node, name).and_then(|child| word_attribute(child, "val"))
}

fn on(node: Xml<'_, '_>, name: &str) -> Option<bool> {
    child(node, name).map(|child| !matches!(word_attribute(child, "val"), Some("0" | "false")))
}

fn number(text: Option<&str>) -> Option<f64> {
    text.and_then(|text| text.parse().ok())
}

/// Sizes are in points.
#[derive(Debug, Clone, Default)]
struct Look {
    based_on: Option<String>,
    values: Map<String, Value>,
}

fn run_look(properties: Xml<'_, '_>, values: &mut Map<String, Value>) {
    if let Some(bold) = on(properties, "b") {
        values.insert("bold".to_owned(), json!(bold));
    }
    if let Some(italic) = on(properties, "i") {
        values.insert("italic".to_owned(), json!(italic));
    }
    if let Some(size) = number(value(properties, "sz")) {
        values.insert("size".to_owned(), json!(size / 2.0));
    }
    if let Some(color) = value(properties, "color").filter(|color| *color != "auto") {
        values.insert("color".to_owned(), json!(color));
    }
    if let Some(font) = child(properties, "rFonts").and_then(|fonts| word_attribute(fonts, "ascii"))
    {
        values.insert("font".to_owned(), json!(font));
    }
}

fn paragraph_look(properties: Xml<'_, '_>, values: &mut Map<String, Value>) {
    for (name, key) in [("keepNext", "keepNext"), ("pageBreakBefore", "pageBreak")] {
        if let Some(set) = on(properties, name) {
            values.insert(key.to_owned(), json!(set));
        }
    }
    if let Some(align) = value(properties, "jc") {
        values.insert("align".to_owned(), json!(align));
    }
    if let Some(spacing) = child(properties, "spacing") {
        for (name, key) in [("before", "before"), ("after", "after"), ("line", "line")] {
            if let Some(amount) = number(word_attribute(spacing, name)) {
                values.insert(key.to_owned(), json!(amount / 20.0));
            }
        }
    }
    if let Some(indent) = child(properties, "ind") {
        for (name, key) in [
            ("left", "indent"),
            ("firstLine", "firstLine"),
            ("hanging", "hanging"),
        ] {
            if let Some(amount) = number(word_attribute(indent, name)) {
                values.insert(key.to_owned(), json!(amount / 20.0));
            }
        }
    }
}

fn resolve(styles: &HashMap<String, Look>, name: &str, depth: usize) -> Map<String, Value> {
    let Some(look) = styles.get(name) else {
        return Map::new();
    };
    let mut values = match &look.based_on {
        Some(base) if depth < 32 => resolve(styles, base, depth + 1),
        _ => Map::new(),
    };
    values.extend(look.values.clone());
    values
}

pub(crate) fn styles(text: &str) -> Value {
    let Ok(parsed) = Document::parse(text) else {
        return Value::Null;
    };
    let root = parsed.root_element();
    let mut defaults = Map::new();
    if let Some(all) = child(root, "docDefaults") {
        if let Some(properties) = child(all, "rPrDefault").and_then(|node| child(node, "rPr")) {
            run_look(properties, &mut defaults);
        }
        if let Some(properties) = child(all, "pPrDefault").and_then(|node| child(node, "pPr")) {
            paragraph_look(properties, &mut defaults);
        }
    }
    let mut looks = HashMap::new();
    let mut default_paragraph = None;
    for style in root.children().filter(|node| is_word(*node, "style")) {
        let Some(id) = word_attribute(style, "styleId") else {
            continue;
        };
        if word_attribute(style, "default").is_some()
            && word_attribute(style, "type") == Some("paragraph")
        {
            default_paragraph = Some(id.to_owned());
        }
        let mut look = Look {
            based_on: value(style, "basedOn").map(str::to_owned),
            values: Map::new(),
        };
        if let Some(properties) = child(style, "rPr") {
            run_look(properties, &mut look.values);
        }
        if let Some(properties) = child(style, "pPr") {
            paragraph_look(properties, &mut look.values);
        }
        look.values
            .insert("name".to_owned(), json!(value(style, "name").unwrap_or(id)));
        look.values.insert(
            "kind".to_owned(),
            json!(word_attribute(style, "type").unwrap_or("paragraph")),
        );
        look.values
            .insert("quick".to_owned(), json!(child(style, "qFormat").is_some()));
        looks.insert(id.to_owned(), look);
    }
    let resolved: Map<String, Value> = looks
        .keys()
        .map(|id| (id.clone(), Value::Object(resolve(&looks, id, 0))))
        .collect();
    json!({
        "defaults": defaults,
        "defaultParagraph": default_paragraph,
        "styles": resolved,
    })
}

pub(crate) fn numbering(text: &str) -> Value {
    let Ok(parsed) = Document::parse(text) else {
        return Value::Null;
    };
    let root = parsed.root_element();
    let mut abstracts = HashMap::new();
    for item in root.children().filter(|node| is_word(*node, "abstractNum")) {
        let Some(id) = word_attribute(item, "abstractNumId") else {
            continue;
        };
        let levels: Vec<Value> = item
            .children()
            .filter(|node| is_word(*node, "lvl"))
            .map(|level| {
                let mut values = Map::new();
                values.insert("format".to_owned(), json!(value(level, "numFmt")));
                values.insert("text".to_owned(), json!(value(level, "lvlText")));
                values.insert("start".to_owned(), json!(number(value(level, "start"))));
                if let Some(properties) = child(level, "pPr") {
                    paragraph_look(properties, &mut values);
                }
                Value::Object(values)
            })
            .collect();
        abstracts.insert(id, levels);
    }
    let lists: Map<String, Value> = root
        .children()
        .filter(|node| is_word(*node, "num"))
        .filter_map(|list| {
            let id = word_attribute(list, "numId")?;
            let levels = abstracts.get(value(list, "abstractNumId")?)?;
            Some((id.to_owned(), Value::Array(levels.clone())))
        })
        .collect();
    Value::Object(lists)
}

/// The page, in points.
pub(crate) fn page(section: Xml<'_, '_>) -> Value {
    let points = |node: Option<Xml<'_, '_>>, name: &str| {
        number(node.and_then(|node| word_attribute(node, name))).map(|value| value / 20.0)
    };
    let size = child(section, "pgSz");
    let margins = child(section, "pgMar");
    json!({
        "width": points(size, "w"),
        "height": points(size, "h"),
        "top": points(margins, "top"),
        "right": points(margins, "right"),
        "bottom": points(margins, "bottom"),
        "left": points(margins, "left"),
    })
}
