use super::diagnostics::SchemaBundle;
use super::keyboard::{replace_selection, utf16_to_byte_index};
use noyalib::{DuplicateKeyPolicy, ParserConfig};
use serde_json::Value;
use std::collections::HashSet;
use wasm_bindgen::JsCast;

#[derive(Clone, PartialEq)]
pub(super) struct Completion {
    pub(super) label: String,
    pub(super) detail: String,
    pub(super) type_label: String,
    pub(super) insert: String,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) cursor_offset: usize,
    pub(super) is_value: bool,
}

fn schema_type_label(schema: &Value, root: &Value, fallback: &str) -> String {
    let mut labels: Vec<String> = resolve_branches(schema, root, 0)
        .into_iter()
        .filter_map(|branch| {
            if branch.get("enum").is_some() {
                return Some("enum".to_string());
            }
            match branch.get("type") {
                Some(Value::String(value)) => Some(value.clone()),
                Some(Value::Array(values)) => {
                    let value = values
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" | ");
                    (!value.is_empty()).then_some(value)
                }
                _ => None,
            }
        })
        .collect();
    labels.sort();
    labels.dedup();
    if labels.is_empty() {
        fallback.to_string()
    } else {
        labels.join(" | ")
    }
}

fn yaml_key_for_completion(key: &str) -> Option<String> {
    let plain_key = !key.is_empty()
        && key
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && key.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
        && !matches!(key.to_ascii_lowercase().as_str(), "true" | "false" | "null");
    if plain_key {
        Some(key.to_string())
    } else {
        serde_json::to_string(key).ok()
    }
}

fn schema_default<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find_map(|branch| branch.get("default"))
}

fn object_branch<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find(|branch| {
            branch.get("properties").is_some()
                || branch.get("type").and_then(Value::as_str) == Some("object")
        })
}

fn append_required_properties(
    schema: &Value,
    root: &Value,
    indent: usize,
    base_offset: usize,
    output: &mut String,
    cursor_offset: &mut Option<usize>,
) -> bool {
    let Some(branch) = object_branch(schema, root) else {
        return false;
    };
    let required = branch.get("required").and_then(Value::as_array);
    let properties = branch.get("properties").and_then(Value::as_object);
    let Some(required) = required else {
        return false;
    };
    let mut appended = false;
    for required_key in required.iter().filter_map(Value::as_str) {
        let Some(property) = properties.and_then(|properties| properties.get(required_key)) else {
            continue;
        };
        let Some(yaml_key) = yaml_key_for_completion(required_key) else {
            continue;
        };
        output.push_str(&" ".repeat(indent));
        output.push_str(&yaml_key);
        output.push(':');
        if let Some(default) = schema_default(property, root) {
            output.push(' ');
            output.push_str(&default.to_string());
            output.push('\n');
        } else if let Some(nested) = object_branch(property, root) {
            let mut children = String::new();
            if append_required_properties(
                nested,
                root,
                indent + 2,
                base_offset + output.encode_utf16().count() + 1,
                &mut children,
                cursor_offset,
            ) {
                output.push('\n');
                output.push_str(&children);
            } else {
                output.push_str(" {}\n");
            }
        } else {
            output.push(' ');
            if cursor_offset.is_none() {
                *cursor_offset = Some(base_offset + output.encode_utf16().count());
            }
            output.push('\n');
        }
        appended = true;
    }
    appended
}

fn property_completion_insert(
    key: &str,
    property: &Value,
    root: &Value,
) -> Option<(String, usize)> {
    let yaml_key = yaml_key_for_completion(key)?;
    let mut insert = format!("{yaml_key}:");
    if let Some(default) = schema_default(property, root) {
        insert.push(' ');
        insert.push_str(&default.to_string());
    } else if let Some(branch) = object_branch(property, root) {
        let mut cursor_offset = None;
        let mut children = String::new();
        if append_required_properties(
            branch,
            root,
            2,
            insert.encode_utf16().count() + 1,
            &mut children,
            &mut cursor_offset,
        ) {
            insert.push('\n');
            insert.push_str(&children);
        } else {
            insert.push_str(" {}");
        }
        let cursor = cursor_offset.unwrap_or_else(|| insert.encode_utf16().count());
        return Some((insert, cursor));
    } else {
        insert.push(' ');
    }
    let cursor = insert.encode_utf16().count();
    Some((insert, cursor))
}
fn yaml_context(source: &str, cursor: usize) -> (Vec<String>, String, usize, bool) {
    let cursor = utf16_to_byte_index(source, cursor);
    let before_cursor = &source[..cursor];
    let current_line_start = before_cursor.rfind('\n').map_or(0, |index| index + 1);
    let current_line = &source[current_line_start..];
    let indent = current_line.len() - current_line.trim_start().len();
    let prefix = current_line.trim_start();

    let mut ancestors: Vec<(usize, String)> = Vec::new();
    for line in source[..current_line_start].lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let line_indent = line.len() - trimmed.len();
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().trim_matches(['\'', '"']);
        if key.is_empty() || key.starts_with('-') {
            continue;
        }
        while ancestors
            .last()
            .is_some_and(|(level, _)| *level >= line_indent)
        {
            ancestors.pop();
        }
        if value.trim().is_empty() {
            ancestors.push((line_indent, key.to_string()));
        }
    }
    while ancestors.last().is_some_and(|(level, _)| *level >= indent) {
        ancestors.pop();
    }
    let path = ancestors.into_iter().map(|(_, key)| key).collect();

    let value_start = prefix
        .find(':')
        .map(|colon| prefix[colon + 1..].len() - prefix[colon + 1..].trim_start().len());
    let in_value = value_start.is_some();
    let prefix_start = if let Some(colon) = prefix.find(':') {
        current_line_start + indent + colon + 1 + value_start.unwrap_or_default()
    } else {
        current_line_start + indent
    };
    let typed = if let Some(colon) = prefix.find(':') {
        prefix[colon + 1..].trim_start()
    } else {
        prefix
    };
    (path, typed.to_string(), prefix_start, in_value)
}

fn resolve_branches<'a>(schema: &'a Value, root: &'a Value, depth: usize) -> Vec<&'a Value> {
    if depth > 16 {
        return Vec::new();
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str)
        && let Some(target) = reference
            .strip_prefix('#')
            .and_then(|pointer| root.pointer(pointer))
    {
        return resolve_branches(target, root, depth + 1);
    }

    let mut branches = vec![schema];
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(options) = schema.get(keyword).and_then(Value::as_array) {
            for option in options {
                branches.extend(resolve_branches(option, root, depth + 1));
            }
        }
    }
    branches
}

fn schema_at_path<'a>(root: &'a Value, path: &[String]) -> Vec<&'a Value> {
    let mut schemas = vec![root];
    for key in path {
        let mut next = Vec::new();
        for current in schemas {
            for branch in resolve_branches(current, root, 0) {
                if let Some(property) = branch
                    .get("properties")
                    .and_then(|properties| properties.get(key))
                {
                    next.push(property);
                }
            }
        }
        schemas = next;
        if schemas.is_empty() {
            break;
        }
    }
    schemas
}

fn existing_mapping_keys(
    source: &str,
    cursor_byte: usize,
    path: &[String],
) -> Option<HashSet<String>> {
    let parser_config =
        ParserConfig::serde_yaml_compat().duplicate_key_policy(DuplicateKeyPolicy::Error);
    let document: Value = noyalib::from_str_with_config(source, &parser_config).ok()?;
    let mapping = path
        .iter()
        .try_fold(&document, |value, key| value.get(key))?
        .as_object()?;
    let current_line = source[..cursor_byte]
        .rsplit('\n')
        .next()
        .unwrap_or_default();
    let current_key = current_line
        .trim_start()
        .split_once(':')
        .map(|(key, _)| key.trim().trim_matches(['\'', '"']).to_string());
    Some(
        mapping
            .keys()
            .filter(|key| current_key.as_deref() != Some(key.as_str()))
            .cloned()
            .collect(),
    )
}

pub(super) fn completions(
    source: &str,
    cursor: usize,
    force: bool,
    schema: Option<&SchemaBundle>,
    enabled: bool,
) -> Vec<Completion> {
    if !enabled {
        return Vec::new();
    }
    let Some(bundle) = schema else {
        return Vec::new();
    };
    let cursor_byte = utf16_to_byte_index(source, cursor);
    let current_line_start = source[..cursor_byte]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let (path, typed, replace_start, in_value) = yaml_context(source, cursor);
    let schemas = schema_at_path(&bundle.schema, &path);

    if in_value {
        let key = source[current_line_start..cursor_byte]
            .trim_start()
            .split_once(':')
            .map(|(key, _)| key.trim().trim_matches(['\'', '"']).to_string());
        let Some(key) = key else {
            return Vec::new();
        };
        let value_schemas = schema_at_path(&bundle.schema, &path_with_key(&path, &key));
        let mut candidates = Vec::new();
        for schema in value_schemas {
            for branch in resolve_branches(schema, &bundle.schema, 0) {
                if let Some(values) = branch.get("enum").and_then(Value::as_array) {
                    for value in values {
                        let label = match value {
                            Value::String(value) => value.clone(),
                            _ => value.to_string(),
                        };
                        if label.starts_with(&typed) {
                            let insert = match value {
                                Value::String(value) => {
                                    serde_json::to_string(value).unwrap_or_else(|_| label.clone())
                                }
                                _ => value.to_string(),
                            };
                            candidates.push(Completion {
                                label: label.clone(),
                                detail: branch
                                    .get("description")
                                    .and_then(Value::as_str)
                                    .unwrap_or("Allowed value")
                                    .to_string(),
                                type_label: schema_type_label(branch, &bundle.schema, "value"),
                                cursor_offset: insert.encode_utf16().count(),
                                insert,
                                start: replace_start,
                                end: cursor,
                                is_value: true,
                            });
                        }
                    }
                }
            }
        }
        return candidates;
    }

    let existing_keys = existing_mapping_keys(source, cursor_byte, &path);
    if (typed.is_empty() && !force)
        || !typed
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    for branch in schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, &bundle.schema, 0))
    {
        if let Some(properties) = branch.get("properties").and_then(Value::as_object) {
            for (key, property) in properties {
                if key.starts_with(&typed)
                    && !existing_keys
                        .as_ref()
                        .is_some_and(|existing| existing.contains(key))
                    && let Some((insert, cursor_offset)) =
                        property_completion_insert(key, property, &bundle.schema)
                {
                    candidates.push(Completion {
                        label: key.clone(),
                        detail: property
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or("Configuration property")
                            .to_string(),
                        type_label: schema_type_label(property, &bundle.schema, "property"),
                        insert,
                        start: replace_start,
                        end: cursor,
                        cursor_offset,
                        is_value: false,
                    });
                }
            }
        }
    }
    candidates.sort_by(|left, right| left.label.cmp(&right.label));
    candidates.dedup_by(|left, right| left.label == right.label);
    candidates
}

fn path_with_key(path: &[String], key: &str) -> Vec<String> {
    let mut path = path.to_vec();
    path.push(key.to_string());
    path
}

pub(super) fn apply_completion(source: &str, completion: &Completion) -> (String, usize) {
    let (updated, end_cursor) =
        replace_selection(source, completion.start, completion.end, &completion.insert);
    let end_offset = completion.insert.encode_utf16().count();
    (
        updated,
        end_cursor
            .saturating_sub(end_offset)
            .saturating_add(completion.cursor_offset),
    )
}

pub(super) fn scroll_completion_into_view(list: &web_sys::HtmlDivElement, index: usize) {
    let Some(option) = list
        .children()
        .item(index as u32)
        .and_then(|option| option.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };

    let top = option.offset_top();
    let bottom = top + option.offset_height();
    let scroll_top = list.scroll_top();
    let visible_bottom = scroll_top + list.client_height();
    if top < scroll_top {
        list.set_scroll_top(top);
    } else if bottom > visible_bottom {
        list.set_scroll_top(bottom - list.client_height());
    }
}

pub(super) fn completion_popup_style(
    editor: &web_sys::HtmlTextAreaElement,
    source: &str,
    cursor: usize,
    option_count: usize,
) -> String {
    let Some(window) = web_sys::window() else {
        return "position:fixed;visibility:hidden".to_string();
    };
    let element: &web_sys::Element = editor.unchecked_ref();
    let rect = element.get_bounding_client_rect();
    let style = window.get_computed_style(element).ok().flatten();
    let css_number = |name: &str, fallback: f64| {
        style
            .as_ref()
            .and_then(|style| style.get_property_value(name).ok())
            .and_then(|value| value.trim_end_matches("px").parse::<f64>().ok())
            .unwrap_or(fallback)
    };
    let font_size = css_number("font-size", 12.0);
    let line_height = css_number("line-height", font_size * 1.8);
    let padding_left = css_number("padding-left", 0.0);
    let padding_top = css_number("padding-top", 0.0);
    let prefix = &source[..utf16_to_byte_index(source, cursor)];
    let (line, column) = prefix
        .rsplit_once('\n')
        .map_or((0, prefix.chars().count()), |(before, after)| {
            (before.matches('\n').count() + 1, after.chars().count())
        });
    let caret_x =
        rect.left() + padding_left + column as f64 * font_size * 0.6 - editor.scroll_left() as f64;
    let caret_y = rect.top() + padding_top + line as f64 * line_height - editor.scroll_top() as f64;
    let viewport_width = window
        .inner_width()
        .ok()
        .and_then(|width| width.as_f64())
        .unwrap_or(1024.0);
    let viewport_height = window
        .inner_height()
        .ok()
        .and_then(|height| height.as_f64())
        .unwrap_or(768.0);
    let width = 420.0_f64.min((viewport_width - 16.0).max(0.0));
    let left = caret_x.clamp(8.0, (viewport_width - width - 8.0).max(8.0));
    let ideal_height = (option_count as f64 * 32.0 + 2.0).min(320.0);
    let space_below = (viewport_height - caret_y - line_height - 8.0).max(0.0);
    let space_above = (caret_y - 8.0).max(0.0);
    let (top, available_height) = if space_below >= ideal_height || space_below >= space_above {
        (caret_y + line_height, space_below)
    } else {
        ((caret_y - ideal_height).max(8.0), space_above)
    };
    let max_height = ideal_height.min(available_height).max(0.0);
    format!(
        "position:fixed;left:{left:.1}px;top:{top:.1}px;width:{width:.1}px;max-height:{max_height:.1}px"
    )
}
