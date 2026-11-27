use std::{
    fs,
    path::{Path, PathBuf},
};

use kodosi_runtime::{Error, Result, protocol};
use serde_json::{Map, Value, json};
use specta::{
    Type, Types,
    datatype::{Attributes, DataType, Fields, NamedReferenceType, Primitive, Reference},
};

#[derive(Clone, Copy)]
enum Direction {
    Input,
    Output,
}

fn shape<T: Type>() -> Result<Value> {
    schema::<T>(Direction::Output)
}

fn schema<T: Type>(direction: Direction) -> Result<Value> {
    let mut types = Types::default();
    let definition = T::definition(&mut types);
    describe(&definition, &types, 0, direction)
}

fn describe(ty: &DataType, types: &Types, depth: usize, direction: Direction) -> Result<Value> {
    if depth > 48 {
        return Err(invalid("recursive protocol type exceeds depth limit"));
    }
    match ty {
        DataType::Primitive(primitive) => Ok(match primitive {
            Primitive::str | Primitive::char => json!({"type":"string"}),
            Primitive::bool => json!({"type":"boolean"}),
            Primitive::f16 | Primitive::f32 | Primitive::f64 | Primitive::f128 => {
                json!({"type":"number"})
            }
            other => json!({"type":"integer", "format":format!("{other:?}")}),
        }),
        DataType::Nullable(inner) => {
            Ok(json!({"anyOf":[describe(inner, types, depth + 1, direction)?, {"type":"null"}]}))
        }
        DataType::List(list) => {
            Ok(json!({"type":"array", "items":describe(&list.ty, types, depth + 1, direction)?}))
        }
        DataType::Struct(structure) => fields(
            &structure.fields,
            &structure.attributes,
            "serde:container:rename_all_serialize",
            types,
            depth + 1,
            direction,
        ),
        DataType::Enum(value) => enumeration(value, types, depth, direction),
        DataType::Reference(Reference::Named(reference)) => {
            let resolved = match &reference.inner {
                NamedReferenceType::Inline { dt, .. } => Some(dt.as_ref()),
                NamedReferenceType::Reference { .. } => {
                    types.get(reference).and_then(|named| named.ty.as_ref())
                }
                NamedReferenceType::Recursive(_) => None,
            }
            .ok_or_else(|| invalid("unresolved protocol type"))?;
            describe(resolved, types, depth + 1, direction)
        }
        other => Err(invalid(format!("unsupported protocol type: {other:?}"))),
    }
}

fn enumeration(
    enumeration: &specta::datatype::Enum,
    types: &Types,
    depth: usize,
    direction: Direction,
) -> Result<Value> {
    let tag = attribute(&enumeration.attributes, "serde:container:tag");
    let content = attribute(&enumeration.attributes, "serde:container:content");
    let untagged = flag(&enumeration.attributes, "serde:container:untagged");
    let mut variants = Vec::new();
    for (name, variant) in &enumeration.variants {
        if variant.skip || flag(&variant.attributes, "serde:variant:skip_serializing") {
            continue;
        }
        let wire = attribute(&variant.attributes, "serde:variant:rename_serialize").map_or_else(
            || {
                rename(
                    name,
                    attribute(
                        &enumeration.attributes,
                        "serde:container:rename_all_serialize",
                    ),
                    true,
                )
            },
            |name| Ok(name.to_owned()),
        )?;
        let rename_fields = attribute(&variant.attributes, "serde:variant:rename_all_serialize")
            .or_else(|| {
                attribute(
                    &enumeration.attributes,
                    "serde:container:rename_all_fields_serialize",
                )
            });
        let mut attributes = variant.attributes.clone();
        if let Some(rule) = rename_fields {
            attributes.insert("wire:fields", rule.to_owned());
        }
        let payload = fields(
            &variant.fields,
            &attributes,
            "wire:fields",
            types,
            depth + 1,
            direction,
        )?;
        variants.push(if untagged {
            payload
        } else if let Some(tag) = tag {
            let tagged = object(
                [(tag.to_owned(), json!({"const":wire}))],
                vec![tag.to_owned()],
            );
            if let Some(content) = content {
                if matches!(variant.fields, Fields::Unit) {
                    tagged
                } else {
                    merge(
                        tagged,
                        object([(content.to_owned(), payload)], vec![content.to_owned()]),
                    )?
                }
            } else if matches!(variant.fields, Fields::Unit) {
                tagged
            } else {
                merge(tagged, payload)?
            }
        } else if matches!(variant.fields, Fields::Unit) {
            json!({"const":wire})
        } else {
            object([(wire.clone(), payload)], vec![wire])
        });
    }
    if variants.is_empty() {
        return Err(invalid("protocol enum has no variants"));
    }
    Ok(json!({"oneOf":variants}))
}

fn fields(
    fields: &Fields,
    attributes: &Attributes,
    rename_key: &str,
    types: &Types,
    depth: usize,
    direction: Direction,
) -> Result<Value> {
    match fields {
        Fields::Unit => Ok(json!({"type":"null"})),
        Fields::Named(named) => {
            let mut base = object([], Vec::new());
            for (name, field) in &named.fields {
                let Some(ty) = &field.ty else {
                    continue;
                };
                if flag(&field.attributes, "serde:field:skip_serializing") {
                    continue;
                }
                let described = describe(ty, types, depth + 1, direction)?;
                if flag(&field.attributes, "serde:field:flatten") {
                    base = merge(base, described)?;
                } else {
                    let name = attribute(&field.attributes, "serde:field:rename_serialize")
                        .map_or_else(
                            || rename(name, attribute(attributes, rename_key), false),
                            |name| Ok(name.to_owned()),
                        )?;
                    let optional = field.optional
                        || match direction {
                            Direction::Input => {
                                flag(&field.attributes, "serde:field:default")
                                    || (matches!(ty, DataType::Nullable(_))
                                        && !flag(
                                            &field.attributes,
                                            "serde:field:has_deserialize_with",
                                        ))
                            }
                            Direction::Output => {
                                attribute(&field.attributes, "serde:field:skip_serializing_if")
                                    .is_some()
                            }
                        };
                    let required = if optional {
                        Vec::new()
                    } else {
                        vec![name.clone()]
                    };
                    base = merge(base, object([(name, described)], required))?;
                }
            }
            Ok(base)
        }
        Fields::Unnamed(unnamed) => {
            let types = unnamed
                .fields
                .iter()
                .filter_map(|field| field.ty.as_ref())
                .map(|ty| describe(ty, types, depth + 1, direction))
                .collect::<Result<Vec<_>>>()?;
            match types.as_slice() {
                [only] => Ok(only.clone()),
                _ => Err(invalid("protocol tuple layout is unsupported")),
            }
        }
    }
}

fn object(fields: impl IntoIterator<Item = (String, Value)>, required: Vec<String>) -> Value {
    let required = Value::Array(required.into_iter().map(Value::String).collect());
    json!({"type":"object", "properties":fields.into_iter().collect::<Map<_, _>>(), "required":required, "additionalProperties":false})
}

fn merge(left: Value, right: Value) -> Result<Value> {
    if let Some(variants) = left.get("oneOf").and_then(Value::as_array) {
        return Ok(
            json!({"oneOf":variants.iter().map(|variant| merge(variant.clone(), right.clone())).collect::<Result<Vec<_>>>()?}),
        );
    }
    if let Some(variants) = right.get("oneOf").and_then(Value::as_array) {
        return Ok(
            json!({"oneOf":variants.iter().map(|variant| merge(left.clone(), variant.clone())).collect::<Result<Vec<_>>>()?}),
        );
    }
    let Value::Object(mut left) = left else {
        return Err(invalid("flatten requires object fields"));
    };
    let Value::Object(mut right) = right else {
        return Err(invalid("flatten requires object fields"));
    };
    let Some(Value::Object(mut properties)) = left.remove("properties") else {
        return Err(invalid("flatten requires object fields"));
    };
    let Some(Value::Object(other_properties)) = right.remove("properties") else {
        return Err(invalid("flatten requires object fields"));
    };
    for (name, value) in other_properties {
        if properties.insert(name.clone(), value).is_some() {
            return Err(invalid(format!("duplicate flattened field {name}")));
        }
    }
    let Some(Value::Array(mut required)) = left.remove("required") else {
        return Err(invalid("object required fields are missing"));
    };
    let Some(Value::Array(other_required)) = right.remove("required") else {
        return Err(invalid("object required fields are missing"));
    };
    required.extend(other_required);
    Ok(
        json!({"type":"object", "properties":properties, "required":required, "additionalProperties":false}),
    )
}

fn attribute<'a>(attributes: &'a Attributes, key: &str) -> Option<&'a str> {
    attributes.get_named_as::<String>(key).map(String::as_str)
}

fn flag(attributes: &Attributes, key: &str) -> bool {
    attributes
        .get_named_as::<bool>(key)
        .copied()
        .unwrap_or(false)
}

fn rename(name: &str, rule: Option<&str>, variant: bool) -> Result<String> {
    match rule {
        None => Ok(name.to_owned()),
        Some("lowercase") => Ok(name.to_ascii_lowercase()),
        Some("camelCase") if variant => {
            let mut chars = name.chars();
            let mut result = chars
                .next()
                .map_or_default(|first| first.to_ascii_lowercase().to_string());
            result.extend(chars);
            Ok(result)
        }
        Some("camelCase") => {
            let mut upper = false;
            let mut result = String::new();
            for ch in name.chars() {
                if ch == '_' {
                    upper = true;
                } else if upper {
                    result.push(ch.to_ascii_uppercase());
                    upper = false;
                } else {
                    result.push(ch);
                }
            }
            Ok(result)
        }
        Some(other) => Err(invalid(format!("unsupported serde rename rule {other}"))),
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

fn rendered() -> Result<String> {
    let value = json!({
        "protocolVersion":protocol::VERSION,
        "ffiAbiVersion":12,
        "commands":schema::<protocol::CommandEnvelope>(Direction::Input)?,
        "events":shape::<protocol::Event>()?,
        "chat":{"pipe":kodosi_runtime::chat::CHAT_PIPE,"frames":shape::<kodosi_runtime::chat::ChatFrame>()?,"requests":schema::<kodosi_runtime::chat::ChatRequest>(Direction::Input)?},
        "video":video(),
        "terminal":{"checkpointSchema":kodosi_runtime::terminal::TERMINAL_CHECKPOINT_SCHEMA_VERSION,"checkpointMaxBytes":kodosi_runtime::terminal::TERMINAL_SNAPSHOT_MAX_BYTES,"imageStorageMaxBytes":kodosi_runtime::terminal::TERMINAL_IMAGE_STORAGE_MAX_BYTES,"rawInputMaxBytes":kodosi_runtime::terminal::TERMINAL_INPUT_MAX_BYTES},
    });
    Ok(serde_json::to_string_pretty(&value)? + "\n")
}

fn video() -> Value {
    use kodosi_runtime::video::Codec;
    json!({
        "codecs":{"h264":Codec::H264.wire(),"hevc":Codec::Hevc.wire()},
        "kinds":{"delta":0,"key":1,"anchor":2},
        "results":{"taken":0,"failed":1},
        "calls":"after kodosi_video_watch the runtime calls on_video_message and on_video_frame of the pipe in pipe order, on a thread of that pipe and never two at once, and on_video_frame only with frames that the decoder can take; the app waits for a slow decoder up to about a second and returns failed only when the decoder cannot go on",
        "acknowledgements":"the runtime acknowledges each frame of the pipe in arrival order: a frame that the decoder cannot take when it arrives, a frame handed to the app when on_video_frame returns; it asks for a keyframe after failed and kodosi_video_restart, and once a second while it waits and frames keep coming",
        "stale":"on_video_stale says true when the picture is not the stream's newest any more, because a repair took longer than a moment or the stream went silent, so the app rests it; false when it is the newest again without a frame; the next on_video_frame ends a rest too",
        "connection":"a desktop app watches each pipe on its own local connection whose hello names the pipe: the host sends the stream's messages as it got them (0 and JSON, 1 and a frame) and 2 with the flag of on_video_stale (1 or 0); the app answers each frame with 3 (taken) or 4 (failed) and sends 5 when its decoder starts over",
    })
}

fn contract_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../protocol/desktop-runtime.json")
}

fn main() -> Result<()> {
    fs::write(contract_file(), rendered()?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "11111111-1111-4111-8111-111111111111";
    #[cfg(unix)]
    const DIRECTORY: &str = "/tmp/project";
    #[cfg(windows)]
    const DIRECTORY: &str = r"C:\project";
    #[cfg(unix)]
    const WORKTREES: &str = "/tmp/branches";
    #[cfg(windows)]
    const WORKTREES: &str = r"C:\branches";

    fn matches(schema: &Value, value: &Value) -> bool {
        if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
            return variants
                .iter()
                .filter(|schema| matches(schema, value))
                .count()
                == 1;
        }
        if let Some(variants) = schema.get("anyOf").and_then(Value::as_array) {
            return variants.iter().any(|schema| matches(schema, value));
        }
        if let Some(expected) = schema.get("const") {
            return value == expected;
        }
        match schema["type"].as_str() {
            Some("object") => {
                let Some(object) = value.as_object() else {
                    return false;
                };
                let properties = schema["properties"].as_object().unwrap();
                schema["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|key| object.contains_key(key.as_str().unwrap()))
                    && object.iter().all(|(key, value)| {
                        properties
                            .get(key)
                            .is_some_and(|schema| matches(schema, value))
                    })
            }
            Some("array") => value
                .as_array()
                .is_some_and(|values| values.iter().all(|value| matches(&schema["items"], value))),
            Some("string") => value.is_string(),
            Some("boolean") => value.is_boolean(),
            Some("integer") => value.is_i64() || value.is_u64(),
            Some("number") => value.is_number(),
            Some("null") => value.is_null(),
            _ => false,
        }
    }

    fn sample(schema: &Value) -> Value {
        if let Some(variants) = schema
            .get("oneOf")
            .or_else(|| schema.get("anyOf"))
            .and_then(Value::as_array)
        {
            return sample(&variants[0]);
        }
        if let Some(value) = schema.get("const") {
            return value.clone();
        }
        match schema["type"].as_str().unwrap() {
            "object" => Value::Object(
                schema["properties"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(key, value)| (key.clone(), sample(value)))
                    .collect(),
            ),
            "array" => json!([sample(&schema["items"])]),
            "string" => json!(ID),
            "boolean" => json!(true),
            "integer" | "number" => json!(1),
            "null" => Value::Null,
            other => panic!("unsupported test shape {other}"),
        }
    }

    fn leaves(schema: &Value) -> Vec<&Value> {
        schema.get("oneOf").and_then(Value::as_array).map_or_else(
            || vec![schema],
            |variants| variants.iter().flat_map(leaves).collect(),
        )
    }

    #[test]
    fn the_contract_file_is_what_the_rust_definitions_give() {
        assert!(
            fs::read_to_string(contract_file()).unwrap() == rendered().unwrap(),
            "protocol/desktop-runtime.json is stale. Run just protocol-gen."
        );
    }

    #[test]
    fn every_event_schema_decodes_and_round_trips_through_serde() {
        let schema = shape::<protocol::Event>().unwrap();
        let variants = leaves(&schema);
        for variant in variants {
            let value = sample(variant);
            let event: protocol::Event = serde_json::from_value(value.clone())
                .unwrap_or_else(|error| panic!("{value}: {error}"));
            let mut encoded = serde_json::to_value(&event).unwrap();
            kodosi_runtime::text::render(&mut encoded, kodosi_runtime::text::Language::English);
            assert!(
                matches(&schema, &encoded),
                "event {} did not match generated schema",
                event.event.kind()
            );
            assert_eq!(encoded["type"], value["type"]);
            assert_eq!(encoded["type"], event.event.kind());
        }
    }

    #[test]
    fn every_command_generated_input_shape_decodes_and_round_trips() {
        let schema = schema::<protocol::CommandEnvelope>(Direction::Input).unwrap();
        for variant in leaves(&schema) {
            let mut value = sample(variant);
            let object = value.as_object_mut().unwrap();
            for (name, replacement) in [
                ("name", json!("Terminal")),
                ("username", json!("example")),
                (
                    "invite",
                    json!(format!("kodosi:example:{}", "ab".repeat(32))),
                ),
                ("workingDir", json!(DIRECTORY)),
                ("workingDirectory", json!(DIRECTORY)),
                ("path", json!(DIRECTORY)),
                ("paths", json!([DIRECTORY])),
                ("folder", json!(DIRECTORY)),
                ("address", json!("https://example.com")),
                ("worktrees", json!(WORKTREES)),
                ("branch", json!("fix/login")),
                ("rows", json!(24)),
                ("cols", json!(80)),
                ("widthPixels", json!(800)),
                ("heightPixels", json!(480)),
                ("cellWidthPixels", json!(10)),
                ("cellHeightPixels", json!(20)),
                ("hardware", json!("02:11:22:33:44:55")),
                (
                    "sleeper",
                    json!({"system":"linux","links":[{"hardware":"02:11:22:33:44:55",
                        "medium":"cable","address":"192.0.2.10","prefix":24,"wakes":"on"}]}),
                ),
            ] {
                if let Some(field) = object.get_mut(name) {
                    *field = replacement;
                }
            }
            if object["type"] == "push.register" {
                object["token"] = json!("ab".repeat(32));
            }
            let command: protocol::CommandEnvelope = serde_json::from_value(value.clone())
                .unwrap_or_else(|error| panic!("{value}: {error}"));
            assert_eq!(value["type"], command.command.operation());
            assert!(matches(&schema, &serde_json::to_value(command).unwrap()));
        }
    }
}
