use std::{fs, path::Path};

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
                .map(|first| first.to_ascii_lowercase().to_string())
                .unwrap_or_default();
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
        "ffiAbiVersion":6,
        "commands":schema::<protocol::CommandEnvelope>(Direction::Input)?,
        "events":shape::<protocol::Event>()?,
        "terminal":{"checkpointSchema":2,"checkpointMaxBytes":kodosi_runtime::terminal::TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES,"rawInputMaxBytes":1_048_576},
    });
    Ok(serde_json::to_string_pretty(&value)? + "\n")
}

fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| invalid("runtime parent missing"))?;
    let path = root.join("protocol/desktop-runtime-authority.json");
    let generated = rendered()?;
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [mode] if mode == "write" => {
            fs::write(path, generated)?;
            Ok(())
        }
        [mode] if mode == "check" => {
            if fs::read_to_string(path)? == generated {
                Ok(())
            } else {
                Err(invalid(
                    "desktop runtime contract is stale; run just protocol-gen",
                ))
            }
        }
        _ => Err(invalid("usage: protocol-gen <write|check>")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    const ID: &str = "11111111-1111-4111-8111-111111111111";

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

    fn check<T: Type + Serialize>(value: &T) {
        let schema = shape::<T>().unwrap();
        let encoded = serde_json::to_value(value).unwrap();
        assert!(
            matches(&schema, &encoded),
            "schema {schema} did not match {encoded}"
        );
    }

    #[test]
    fn every_event_schema_decodes_and_round_trips_through_serde() {
        let schema = shape::<protocol::Event>().unwrap();
        let variants = leaves(&schema);
        assert_eq!(variants.len(), 36);
        for variant in variants {
            let value = sample(variant);
            let event: protocol::Event = serde_json::from_value(value.clone())
                .unwrap_or_else(|error| panic!("{value}: {error}"));
            let encoded = serde_json::to_value(&event).unwrap();
            assert!(
                matches(&schema, &encoded),
                "event {} did not match generated schema",
                event.kind()
            );
            assert_eq!(encoded["type"], value["type"]);
        }
    }

    #[test]
    fn serde_renaming_flattening_and_required_fields_are_preserved() {
        check(&protocol::CommandEnvelope {
            account_user_id: None,
            account_epoch: 0,
            command: protocol::Command::ListSessions {},
        });
        check(&protocol::CommandEnvelope {
            account_user_id: None,
            account_epoch: 0,
            command: protocol::Command::Resize {
                session_id: ID.to_owned(),
                claim: true,
                identity: protocol::ResizeIdentity {
                    request_id: ID.to_owned(),
                    expected_runtime_incarnation_id: ID.to_owned(),
                    subscription_id: ID.to_owned(),
                    subscription_generation: 1,
                    surface_generation: 1,
                    cols: 80,
                    rows: 24,
                    width_pixels: 800,
                    height_pixels: 480,
                    cell_width_pixels: 10,
                    cell_height_pixels: 20,
                },
            },
        });
        let schema = shape::<protocol::CommandEnvelope>().unwrap();
        let resized = leaves(&schema)
            .into_iter()
            .find(|schema| schema["properties"]["type"]["const"] == "session.resize")
            .unwrap();
        assert!(resized["properties"].get("identity").is_none());
        assert_eq!(
            resized["properties"]["subscriptionGeneration"]["type"],
            "integer"
        );
        assert!(
            resized["required"]
                .as_array()
                .unwrap()
                .contains(&json!("subscriptionGeneration"))
        );
        let mut malformed = serde_json::to_value(protocol::CommandEnvelope {
            account_user_id: None,
            account_epoch: 0,
            command: protocol::Command::ListSessions {},
        })
        .unwrap();
        malformed["unexpected"] = json!(true);
        assert!(!matches(&schema, &malformed));
        assert!(serde_json::from_value::<protocol::CommandEnvelope>(malformed).is_err());
    }

    #[test]
    fn provider_nullable_fields_are_not_accidentally_optional() {
        let info = kodosi_runtime::provider::ProviderInfo {
            provider: kodosi_runtime::provider::Provider::Claude,
            executable: None,
            files: vec![],
            message: None,
        };
        check(&info);
        let schema = shape::<kodosi_runtime::provider::ProviderInfo>().unwrap();
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("executable"))
        );
        assert!(matches(&schema["properties"]["executable"], &Value::Null));
        assert!(schema["properties"].get("version").is_none());
        let provider = shape::<kodosi_runtime::provider::Provider>().unwrap();
        assert!(matches(&provider, &json!("claude")));
        assert!(!matches(&provider, &json!("Claude")));
    }

    #[test]
    fn omitted_optional_fields_match_actual_event_serialization() {
        let event =
            protocol::Event::new(None, 0, json!({"type":"auth.ready","userId":ID})).unwrap();
        check(&event);
        let schema = shape::<protocol::EventBody>().unwrap();
        let ready = leaves(&schema)
            .into_iter()
            .find(|schema| schema["properties"]["type"]["const"] == "auth.ready")
            .unwrap();
        assert!(
            !ready["required"]
                .as_array()
                .unwrap()
                .contains(&json!("displayName"))
        );
        assert!(matches(
            &shape::<protocol::SessionKind>().unwrap(),
            &json!("remote")
        ));
        assert!(matches(
            &shape::<protocol::SessionStatus>().unwrap(),
            &json!("reconnecting")
        ));
        assert!(matches(
            &shape::<protocol::AuthRequiredReason>().unwrap(),
            &json!("signedOut")
        ));
    }

    #[test]
    fn optional_command_input_is_distinct_from_nullable_event_output() {
        let schema = schema::<protocol::CommandEnvelope>(Direction::Input).unwrap();
        let minimal = json!({"type":"session.create","requestId":ID,"name":"Terminal","accountUserId":null,"accountEpoch":0});
        assert!(matches(&schema, &minimal));
        assert!(serde_json::from_value::<protocol::CommandEnvelope>(minimal.clone()).is_ok());
        let mut missing_account = minimal;
        missing_account
            .as_object_mut()
            .unwrap()
            .remove("accountUserId");
        assert!(!matches(&schema, &missing_account));
        assert!(serde_json::from_value::<protocol::CommandEnvelope>(missing_account).is_err());
        let create = leaves(&schema)
            .into_iter()
            .find(|value| value["properties"]["type"]["const"] == "session.create")
            .unwrap();
        assert!(
            !create["required"]
                .as_array()
                .unwrap()
                .contains(&json!("workingDir"))
        );
        assert!(
            !create["required"]
                .as_array()
                .unwrap()
                .contains(&json!("resume"))
        );
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
                ("slug", json!("release")),
                ("workingDir", json!("/tmp/project")),
                ("workingDirectory", json!("/tmp/project")),
                ("rows", json!(24)),
                ("cols", json!(80)),
                ("widthPixels", json!(800)),
                ("heightPixels", json!(480)),
                ("cellWidthPixels", json!(10)),
                ("cellHeightPixels", json!(20)),
            ] {
                if let Some(field) = object.get_mut(name) {
                    *field = replacement;
                }
            }
            let command: protocol::CommandEnvelope = serde_json::from_value(value.clone())
                .unwrap_or_else(|error| panic!("{value}: {error}"));
            assert!(matches(&schema, &serde_json::to_value(command).unwrap()));
        }
    }
}
