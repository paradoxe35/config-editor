//! Elements with a prefix (`<mvc:interceptor>`, `<xs:element>`,
//! `<a:item xmlns:a="…">`) edited in another view: the model keys them by
//! their local name, yet a save keeps every element in its namespace, and
//! an item added takes its siblings' (QA `E32-1`: adding an item to
//! `<mvc:interceptors>` saved every `<mvc:interceptor>` as
//! `<interceptor>`, and an item added under `<xs:schema>` could never
//! save). Each saved element's expanded name (namespace and local name),
//! as a namespace-aware reader reads it, is checked against the host's.

use crate::editor::Language;
use crate::editor::save_matrix::{report, save_value};
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;
use serde_json::{Value, json};
use std::collections::HashMap;

const VIEWS: [Language; 4] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
];

/// A host: its text with `count` items, the list's pointer and a new item.
struct Shape {
    label: &'static str,
    host: fn(usize) -> String,
    list: &'static str,
    fresh: fn() -> Value,
}

fn items(count: usize, item: impl Fn(usize) -> String) -> String {
    (0..count).map(item).collect()
}

const SHAPES: [Shape; 5] = [
    // A default namespace, the items prefixed (Spring `context:`).
    Shape {
        label: "spring context",
        host: |count| {
            format!(
                "<beans xmlns=\"http://www.springframework.org/schema/beans\"\n       xmlns:context=\"http://www.springframework.org/schema/context\">\n{}  <bean id=\"a\" class=\"A\"/>\n</beans>\n",
                items(count, |item| format!(
                    "  <context:property-placeholder location=\"p{item}.properties\"/>\n"
                ))
            )
        },
        list: "/beans/property-placeholder",
        fresh: || json!({"location": "new.properties"}),
    },
    // A prefixed parent holding prefixed items, with children of the
    // default namespace (Spring `mvc:interceptors`).
    Shape {
        label: "spring mvc",
        host: |count| {
            format!(
                "<beans xmlns=\"http://www.springframework.org/schema/beans\"\n       xmlns:mvc=\"http://www.springframework.org/schema/mvc\">\n  <mvc:interceptors>\n{}  </mvc:interceptors>\n</beans>\n",
                items(count, |item| format!(
                    "    <mvc:interceptor>\n      <mvc:mapping path=\"/p{item}/**\"/>\n      <bean class=\"C{item}\"/>\n    </mvc:interceptor>\n"
                ))
            )
        },
        list: "/beans/interceptors/interceptor",
        fresh: || json!({"mapping": {"path": "/new/**"}, "bean": {"class": "N"}}),
    },
    // Everything prefixed, the root too (an XSD).
    Shape {
        label: "xsd",
        host: |count| {
            format!(
                "<?xml version=\"1.0\"?>\n<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\">\n{}</xs:schema>\n",
                items(count, |item| format!(
                    "  <xs:element name=\"e{item}\" type=\"xs:string\"/>\n"
                ))
            )
        },
        list: "/schema/element",
        fresh: || json!({"name": "n", "type": "xs:int"}),
    },
    // Each item declares its own prefix.
    Shape {
        label: "own declarations",
        host: |count| {
            format!(
                "<list>\n{}</list>\n",
                items(count, |item| format!(
                    "  <a:item xmlns:a=\"urn:one\">\n    <v>{item}</v>\n  </a:item>\n"
                ))
            )
        },
        list: "/list/item",
        fresh: || json!({"v": 9}),
    },
    // A default namespace and a prefix, items holding both kinds of child
    // (an NLog-like file).
    Shape {
        label: "mixed",
        host: |count| {
            format!(
                "<nlog xmlns=\"urn:nlog\" xmlns:x=\"urn:extra\">\n  <x:rules>\n{}  </x:rules>\n</nlog>\n",
                items(count, |item| format!(
                    "    <x:rule name=\"r{item}\">\n      <x:level>{item}</x:level>\n      <target>t{item}</target>\n    </x:rule>\n"
                ))
            )
        },
        list: "/nlog/rules/rule",
        fresh: || json!({"name": "n", "level": 9, "target": "n"}),
    },
];

/// An edit of the list (its items as the view shows them), on a host of
/// how many items, and whether it renames the list's key.
struct Operation {
    label: &'static str,
    count: usize,
    edit: fn(&mut Vec<Value>, &Value),
    renames: bool,
}

const OPERATIONS: [Operation; 10] = [
    Operation {
        label: "add a key to an item",
        count: 3,
        edit: |items, _| {
            if let Some(map) = items[1].as_object_mut() {
                map.insert("extra".into(), json!("x"));
            }
        },
        renames: false,
    },
    Operation {
        label: "add an item first",
        count: 3,
        edit: |items, fresh| items.insert(0, fresh.clone()),
        renames: false,
    },
    Operation {
        label: "add an item in the middle",
        count: 3,
        edit: |items, fresh| items.insert(1, fresh.clone()),
        renames: false,
    },
    Operation {
        label: "add an item last",
        count: 3,
        edit: |items, fresh| items.push(fresh.clone()),
        renames: false,
    },
    Operation {
        label: "make a lone item a list",
        count: 1,
        edit: |items, fresh| items.push(fresh.clone()),
        renames: false,
    },
    Operation {
        label: "delete an item",
        count: 3,
        edit: |items, _| {
            items.remove(1);
        },
        renames: false,
    },
    Operation {
        label: "move an item",
        count: 3,
        edit: |items, _| items.swap(0, 1),
        renames: false,
    },
    Operation {
        label: "duplicate an item",
        count: 3,
        edit: |items, _| {
            let copy = items[1].clone();
            items.insert(2, copy);
        },
        renames: false,
    },
    Operation {
        label: "change every item",
        count: 3,
        edit: |items, fresh| {
            for item in items.iter_mut() {
                *item = fresh.clone();
            }
        },
        renames: false,
    },
    Operation {
        label: "rename the list",
        count: 3,
        edit: |_, _| {},
        renames: true,
    },
];

/// Each element's expanded name, as a namespace-aware reader reads it, by
/// the local names of its path, in document order.
fn expanded_names(text: &str) -> Result<Vec<(Vec<String>, String)>, String> {
    let mut reader = NsReader::from_str(text);
    let mut path: Vec<String> = Vec::new();
    let mut names = Vec::new();
    loop {
        let (resolved, event) = reader
            .read_resolved_event()
            .map_err(|error| error.to_string())?;
        let (start, closes) = match &event {
            Event::Start(start) => (start, false),
            Event::Empty(start) => (start, true),
            Event::End(_) => {
                path.pop();
                continue;
            }
            Event::Eof => return Ok(names),
            _ => continue,
        };
        let namespace = match resolved {
            ResolveResult::Bound(namespace) => namespace.0.to_string(),
            ResolveResult::Unbound => String::new(),
            ResolveResult::Unknown(prefix) => {
                return Err(format!("unbound prefix {prefix:?}"));
            }
        };
        path.push(start.local_name().into_inner().to_string());
        names.push((path.clone(), namespace));
        if closes {
            path.pop();
        }
    }
}

/// `value` with the last key of `pointer` renamed `to`, in its place.
fn renamed(value: &mut Value, pointer: &str, to: &str) {
    let (parent, key) = pointer.rsplit_once('/').expect("a key");
    let Some(map) = value.pointer_mut(parent).and_then(Value::as_object_mut) else {
        return;
    };
    *map = std::mem::take(map)
        .into_iter()
        .map(|(held, child)| (if held == key { to.to_string() } else { held }, child))
        .collect();
}

#[test]
fn prefixed_elements_keep_their_namespace_through_every_view_edit() {
    let mut failures = Vec::new();
    for shape in &SHAPES {
        for operation in &OPERATIONS {
            let host = (shape.host)(operation.count);
            let before = expanded_names(&host).expect("the host reads");
            // The namespace of each element path the host holds.
            let mut namespaces: HashMap<Vec<String>, String> = HashMap::new();
            for (path, namespace) in &before {
                namespaces.insert(path.clone(), namespace.clone());
            }
            let list_key = shape.list.rsplit('/').next().expect("a key");
            let renamed_key = format!("{list_key}s2");
            for edit in VIEWS {
                let case = format!("{}, {}, via {edit}", shape.label, operation.label);
                let saved = save_value(Language::XML, edit, &host, &|value| {
                    if operation.renames {
                        renamed(value, shape.list, &renamed_key);
                        return;
                    }
                    let Some(slot) = value.pointer_mut(shape.list) else {
                        return;
                    };
                    let mut items = match slot.take() {
                        Value::Array(items) => items,
                        lone => vec![lone],
                    };
                    (operation.edit)(&mut items, &(shape.fresh)());
                    *slot = if items.len() == 1 {
                        items.remove(0)
                    } else {
                        Value::Array(items)
                    };
                });
                let saved = match saved {
                    Ok(saved) => saved,
                    Err(error) => {
                        failures.push(format!("{case}: not saved: {error}"));
                        continue;
                    }
                };
                let after = match expanded_names(&saved) {
                    Ok(after) => after,
                    Err(error) => {
                        failures.push(format!("{case}: {error}:\n{saved}"));
                        continue;
                    }
                };
                for (path, namespace) in &after {
                    // A renamed list is in the namespace it was in.
                    let as_host: Vec<String> = path
                        .iter()
                        .map(|local| {
                            if *local == renamed_key {
                                list_key.to_string()
                            } else {
                                local.clone()
                            }
                        })
                        .collect();
                    // A key the edit added has no host namespace to keep.
                    let Some(wanted) = namespaces.get(&as_host) else {
                        continue;
                    };
                    if namespace != wanted {
                        failures.push(format!(
                            "{case}: {} in {namespace:?}, wanted {wanted:?}:\n{saved}",
                            path.join("/")
                        ));
                        break;
                    }
                }
            }
        }
    }
    report(failures);
}
