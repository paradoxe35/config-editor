//! Host diagnostics (the `hostDiagnostics` option and `update` key) from JavaScript.

use config_editor_leptos::{HostDiagnostic, Severity};
use js_sys::Array;
use wasm_bindgen::prelude::*;

use crate::options::warn_unknown_keys;
use crate::values::{field, path_from_js, range_from_js, string_field};

/// Host diagnostics from JavaScript; `host_text` (the document in
/// `language`) turns string-index ranges into lines and columns.
pub(crate) fn host_diagnostics_from(
    list: &JsValue,
    host_text: &str,
) -> Result<Vec<HostDiagnostic>, JsError> {
    if !Array::is_array(list) {
        return Err(JsError::new(
            "hostDiagnostics must be an array of { path | range, severity, message, code?, source?, docsUrl? }",
        ));
    }
    Array::from(list)
        .iter()
        .enumerate()
        .map(|(index, item)| {
            host_diagnostic_from(&item, host_text)
                .map_err(|error| JsError::new(&format!("hostDiagnostics[{index}]: {error}")))
        })
        .collect()
}

/// Keys a host diagnostic can hold.
pub(crate) const HOST_DIAGNOSTIC_KEYS: &[&str] = &[
    "path", "range", "severity", "message", "code", "source", "docsUrl",
];

pub(crate) fn host_diagnostic_from(
    item: &JsValue,
    host_text: &str,
) -> Result<HostDiagnostic, String> {
    if !item.is_object() || Array::is_array(item) {
        return Err("must be an object".to_string());
    }
    warn_unknown_keys(item, HOST_DIAGNOSTIC_KEYS, "host diagnostic key");
    let message = string_field(item, "message").ok_or("message must be a string")?;
    if message.trim().is_empty() {
        return Err("message must not be empty".to_string());
    }
    let severity = match field(item, "severity") {
        value if value.is_undefined() || value.is_null() => Severity::Error,
        value => severity_from(value.as_string())?,
    };
    let path = field(item, "path");
    let range = field(item, "range");
    let given = |value: &JsValue| !value.is_undefined() && !value.is_null();
    if given(&path) && given(&range) {
        return Err("give a path or a range, not both".to_string());
    }
    let mut diagnostic = if !path.is_undefined() && !path.is_null() {
        let path = path_from_js(&path).map_err(|error| {
            format!(
                "path: {}",
                js_sys::Error::from(JsValue::from(error)).message()
            )
        })?;
        HostDiagnostic::at_path(path, severity, message)
    } else if !range.is_undefined() && !range.is_null() {
        HostDiagnostic::at_range(range_from_js(&range, host_text)?, severity, message)
    } else {
        HostDiagnostic::document(severity, message)
    };
    let optional = |name: &str| -> Result<Option<String>, String> {
        let value = field(item, name);
        if value.is_undefined() || value.is_null() {
            return Ok(None);
        }
        value
            .as_string()
            .map(Some)
            .ok_or(format!("{name} must be a string"))
    };
    diagnostic.code = optional("code")?;
    diagnostic.source = optional("source")?;
    if diagnostic
        .source
        .as_deref()
        .is_some_and(|source| source.trim().is_empty())
    {
        return Err("source must name who reported the problem (got an empty string)".to_string());
    }
    diagnostic.docs_url = optional("docsUrl")?;
    if let Some(url) = &diagnostic.docs_url
        && !config_editor_leptos::js_support::is_web_url(url)
    {
        return Err(format!(
            "docsUrl must be an http(s) URL with a host (got {url:?})"
        ));
    }
    Ok(diagnostic)
}

/// The severities a host diagnostic takes, written as the typings name them.
const SEVERITIES: [(&str, Severity); 4] = [
    ("error", Severity::Error),
    ("warning", Severity::Warning),
    ("info", Severity::Info),
    ("hint", Severity::Hint),
];

/// A severity from JavaScript: exactly one of the lower-case names
/// (`"ERROR"` or `" error"` is a mistake the host should hear about).
fn severity_from(name: Option<String>) -> Result<Severity, String> {
    let name = name.ok_or("severity must be a string")?;
    SEVERITIES
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, severity)| *severity)
        .ok_or_else(|| {
            format!(
                "unknown severity {name:?} (expected \"error\", \"warning\", \"info\" or \"hint\", lower-case)"
            )
        })
}
