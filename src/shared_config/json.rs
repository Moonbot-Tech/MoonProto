//! Typed JSON access for automation clients without a second, drifting settings catalog.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use serde_value::Value as TypedValue;

use super::{SharedConfig, SharedConfigError};

impl SharedConfig {
    /// Schema derived from the same types as the wire codec, with MoonDoc links and numeric ranges.
    /// Runtime support and trading meaning still come from the connected core and MoonDoc.
    pub fn json_schema() -> Value {
        serde_json::to_value(schemars::schema_for!(SharedConfig)).expect("JSON schema is serializable")
    }

    /// Known settings in their native types. Opaque future section bytes are never exposed.
    /// Service version fields remain visible but are read-only in [`Self::patch_json`].
    pub fn json_values(&self) -> Result<Value, SharedConfigError> {
        let mut value = serde_json::to_value(self).map_err(json_error)?;
        if let Some(sections) = value.as_object_mut() {
            for section in sections.values_mut().filter_map(Value::as_object_mut) {
                section.remove("unknown_tail");
            }
        }
        Ok(value)
    }

    /// Apply exact dot-separated field paths to a fresh complete core snapshot.
    /// Only leaf values or complete arrays may be replaced. Unknown fields, numeric overflow,
    /// wrong types and fixed-array length changes fail without modifying the source. Unchanged
    /// fields, service versions and opaque future bytes survive. This is not a server-side CAS.
    pub fn patch_json(&self, changes: &BTreeMap<String, Value>) -> Result<Self, SharedConfigError> {
        // JSON maps nonfinite floats to null. Keep the untouched wire snapshot lossless.
        let mut value = serde_value::to_value(self).map_err(json_error)?;
        for (path, replacement) in changes {
            let current = setting_value(&mut value, path)?;
            if matches!(current, TypedValue::Map(_)) {
                return Err(SharedConfigError::new(format!("select individual fields inside {path}")));
            }
            *current = serde_value::to_value(replacement).map_err(json_error)?;
        }
        let changed: Self = value.deserialize_into().map_err(json_error)?;
        let mut typed = serde_value::to_value(&changed).map_err(json_error)?;
        for path in changes.keys() {
            // Check after typing: a finite JSON f64 can overflow a destination f32.
            if !finite_value(setting_value(&mut typed, path)?) {
                return Err(SharedConfigError::new(format!("nonfinite or overflowing setting: {path}")));
            }
        }
        Ok(changed)
    }
}

/// Resolve an editable leaf without admitting service metadata or array-index paths.
fn setting_value<'a>(mut value: &'a mut TypedValue, path: &str) -> Result<&'a mut TypedValue, SharedConfigError> {
    for part in path.split('.') {
        if matches!(part, "unknown_tail" | "config_version" | "ver" | "filled") {
            return Err(SharedConfigError::new(format!("read-only setting: {path}")));
        }
        value = match value {
            TypedValue::Map(object) => object.get_mut(&TypedValue::String(part.into())),
            _ => None,
        }.ok_or_else(|| SharedConfigError::new(format!("unknown setting: {path}")))?;
    }
    Ok(value)
}

/// Reject newly introduced nonfinite floats while preserving them in untouched fields.
fn finite_value(value: &TypedValue) -> bool {
    match value {
        TypedValue::F32(value) => value.is_finite(),
        TypedValue::F64(value) => value.is_finite(),
        TypedValue::Seq(values) => values.iter().all(finite_value),
        TypedValue::Map(values) => values.values().all(finite_value),
        _ => true,
    }
}

/// Preserve the crate's existing error contract without exposing a new error hierarchy.
fn json_error(error: impl std::fmt::Display) -> SharedConfigError {
    SharedConfigError::new(format!("invalid settings JSON: {error}"))
}

/// Serde's built-in arrays stop at 32; the wire catalog also contains 41- and 51-element arrays.
pub(super) mod array {
    use super::*;

    /// Serialize the fixed array without copying it into another container.
    pub(in crate::shared_config) fn serialize<T: Serialize, S: Serializer, const N: usize>(value: &[T; N], serializer: S) -> Result<S::Ok, S::Error> {
        value.as_slice().serialize(serializer)
    }

    /// Require exactly the wire array's size, including when an automation client changes it.
    pub(in crate::shared_config) fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>, const N: usize>(deserializer: D) -> Result<[T; N], D::Error> {
        let values = Vec::<T>::deserialize(deserializer)?;
        values.try_into().map_err(|values: Vec<T>| serde::de::Error::custom(format!("expected {N} values, got {}", values.len())))
    }
}

#[cfg(test)]
mod tests;
