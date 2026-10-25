//! Bounded direct user-property links; no script execution or asset I/O.
use super::{InspectionError, text};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(super) fn validate(properties: &BTreeMap<String, Value>) -> Result<(), InspectionError> {
    if properties.len() > 128
        || properties.iter().any(|(key, value)| {
            key.is_empty()
                || key.len() > 64
                || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                || match value {
                    Value::Bool(_) => false,
                    Value::String(value) => value.len() > 256,
                    Value::Number(value) => value.as_f64().is_none_or(|v| !v.is_finite()),
                    _ => true,
                }
        })
    {
        return Err(InspectionError::invalid(
            "user properties",
            "expected at most 128 named Boolean, finite number or bounded string values",
        ));
    }
    Ok(())
}

pub(super) fn resolve(
    value: &mut Value,
    field: &str,
    context: &str,
    properties: &BTreeMap<String, Value>,
    tolerant: bool,
    resolved: &mut Vec<String>,
    omissions: &mut Vec<String>,
) -> Result<bool, InspectionError> {
    if field == "text"
        && text::known_script(value)
        && let Some(options) = value
            .get_mut("scriptproperties")
            .and_then(Value::as_object_mut)
    {
        for (name, option) in options {
            resolve(
                option,
                name,
                &format!("{context} text option {name}"),
                properties,
                tolerant,
                resolved,
                omissions,
            )?;
        }
    }
    let Some(default) = value.get("value").cloned() else {
        return Ok(false);
    };
    let identity = value
        .get("script")
        .and_then(Value::as_str)
        .is_some_and(|script| {
            format!("{:x}", Sha256::digest(script.as_bytes()))
                == "66403172f254a269b9ff2762ef98dd08cb9b8c35c34238255920fd69708f086e"
        });
    if field == "text" && text::supported(value) {
        resolved.push(format!(
            "{context}: text uses a checked clock/calendar profile."
        ));
        return Ok(true);
    }
    let user = value.get("user");
    let link = user.and_then(|user| {
        user.as_str().map(|name| (name, None)).or_else(|| {
            let map = user.as_object()?;
            (map.len() == 2).then_some(())?;
            Some((
                map.get("name")?.as_str()?,
                Some(map.get("condition")?.as_str()?),
            ))
        })
    });
    let script_supported = value.get("script").is_none() || identity;
    let boolean = matches!(
        field,
        "visible" | "use24hFormat" | "showSeconds" | "showDay" | "useDelimiter" | "alignVertical"
    );
    let string = matches!(
        field,
        "text" | "color" | "delimiter" | "addDelimiter" | "dayFormat" | "monthFormat"
    );
    if let Some((name, condition)) = link.filter(|(name, condition)| {
        script_supported
            && (boolean || string)
            && !name.is_empty()
            && name.len() <= 64
            && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            && (condition.is_none() || field == "visible")
    }) {
        let bound = properties.get(name);
        let replacement = match (bound, condition) {
            (Some(Value::String(actual)), Some(expected)) if field == "visible" => {
                Value::Bool(actual == expected)
            }
            (Some(_), Some(_)) => {
                return Err(InspectionError::invalid(
                    context,
                    "visibility combo property requires a string",
                ));
            }
            (Some(actual), None) if actual.is_boolean() && boolean => actual.clone(),
            (Some(actual), None) if actual.is_string() && string => actual.clone(),
            (Some(_), None) => {
                return Err(InspectionError::invalid(
                    context,
                    format!("invalid user property {name} for {field}"),
                ));
            }
            (None, _) => default,
        };
        if field == "text" {
            text::checked_string(
                replacement.as_str().ok_or_else(|| {
                    InspectionError::invalid(context, "text binding requires a string")
                })?,
                context,
            )?;
        }
        if field == "color" {
            let color = super::snapshot::vector(Some(&replacement), [1.; 3], context)?;
            if color.iter().any(|v| !(0. ..=1.).contains(v)) {
                return Err(InspectionError::invalid(
                    context,
                    "color binding requires components in 0..=1",
                ));
            }
        }
        resolved.push(format!("{context}: {field} uses user property {name}."));
        *value = replacement;
        return Ok(true);
    }
    fallback(value, default, field, context, tolerant, omissions)
}

fn fallback(
    value: &mut Value,
    default: Value,
    field: &str,
    context: &str,
    tolerant: bool,
    omissions: &mut Vec<String>,
) -> Result<bool, InspectionError> {
    if !tolerant {
        return Err(InspectionError::invalid(
            context,
            format!("unsupported {field} binding"),
        ));
    }
    omissions.push(format!("{context}: {field} binding uses its authored default; interactive/script updates are omitted."));
    *value = default;
    Ok(false)
}
