//! The other files a schema reads (`xs:include`, `xs:import`,
//! `xs:redefine`), from the host's resources: nothing is fetched.

use std::collections::HashSet;

use super::super::super::diagnostics::SchemaError;
use super::super::SchemaResources;
use super::super::bundle::{join, resource};
use super::compile::error;
use super::tree::{self, Node};

/// The namespace of `xml:lang` and `xml:space`, whose schema every reader
/// knows.
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// Every file `main` (at `location`) reads, and the files they read, each
/// once, in the order first named. A location resolves against the file
/// naming it; its text is the resource at the resolved location, else at
/// the location as written; an import without a location is the resource
/// `urn:` and its namespace.
pub(super) fn load(
    main: &Node,
    location: Option<&str>,
    resources: &SchemaResources,
) -> Result<Vec<Node>, SchemaError> {
    let mut files: Vec<Node> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    if let Some(location) = location {
        seen.insert(location.to_string());
    }
    let mut queue: Vec<(String, String, String)> = directives(main, location);
    let mut next = 0;
    while next < queue.len() {
        let (resolved, written, at) = queue[next].clone();
        next += 1;
        if !seen.insert(resolved.clone()) {
            continue;
        }
        let text = resource(resources, &resolved, &written)
            .map_err(|missing| SchemaError(format!("{} ({at})", missing.0)))?;
        let file = tree::read_at(text, &written)?;
        if !file.is("schema") {
            return Err(error(&file, format!("{written} is no XML Schema")));
        }
        queue.extend(directives(&file, Some(&resolved)));
        files.push(file);
    }
    Ok(files)
}

/// The files `schema` (at `base`) names: each one's resolved location,
/// its location as written, and where it is named.
fn directives(schema: &Node, base: Option<&str>) -> Vec<(String, String, String)> {
    let place = |node: &Node| {
        let file = node
            .location
            .as_deref()
            .map_or_else(String::new, |location| format!("{location}, "));
        format!("{file}line {}, column {}", node.line, node.column)
    };
    schema
        .xsd_children()
        .filter(|node| matches!(node.name.as_str(), "include" | "import" | "redefine"))
        .filter_map(|node| match node.attribute("schemaLocation") {
            Some(written) => Some((join(base, written), written.to_string(), place(node))),
            None => {
                let namespace = node.attribute("namespace")?;
                (namespace != XML_NAMESPACE).then(|| {
                    let key = if namespace.starts_with("urn:") {
                        namespace.to_string()
                    } else {
                        format!("urn:{namespace}")
                    };
                    (key.clone(), key, place(node))
                })
            }
        })
        .collect()
}
