//! Objects of many entries written in time growing with their entries.
//!
//! `hcl-rs` builds an object expression by checking each key against every
//! key before it, so writing a map of thousands of entries (a big `locals`
//! map saved from another view) took time growing with the square of its
//! entries. While a document is written, such an object is written entry
//! by entry (each one as the formatter writes it alone, in an object of
//! its own) and stands in the document as a name the text is then put in
//! place of, indented where it stands. A stand-in that is not found once,
//! or a heredoc in an entry (its lines must not be indented), writes the
//! document again the whole way.

use std::cell::RefCell;

use hcl::expr::{Expression, Object, ObjectKey, Variable};
use serde_json::{Map, Value};

/// An object of more entries is written entry by entry.
const ENTRIES: usize = 128;

/// The objects written entry by entry while a document is written: each
/// one's stand-in and its text (`{`, its entries, `}`, at the top level).
#[derive(Default)]
struct Written {
    objects: Vec<(String, String)>,
    /// An entry the stand-ins cannot hold (a heredoc).
    whole: bool,
}

thread_local! {
    static WRITING: RefCell<Option<Written>> = const { RefCell::new(None) };
}

/// The document `render` writes, each object of many entries in it written
/// entry by entry.
pub(super) fn writing(render: impl Fn() -> Result<String, String>) -> Result<String, String> {
    let previous = WRITING.with(|writing| writing.borrow_mut().replace(Written::default()));
    let rendered = render();
    let written = WRITING.with(|writing| std::mem::replace(&mut *writing.borrow_mut(), previous)).unwrap_or_default();
    let rendered = rendered?;
    if written.objects.is_empty() {
        return Ok(rendered);
    }
    match (!written.whole).then(|| put_in_place(&rendered, &written.objects)).flatten() {
        Some(text) => Ok(text),
        // Written whole, as the formatter writes it.
        None => {
            let previous = WRITING.with(|writing| writing.borrow_mut().take());
            let rendered = render();
            WRITING.with(|writing| *writing.borrow_mut() = previous);
            rendered
        }
    }
}

/// `map` as an expression standing for it, written entry by entry with
/// `entry` writing each value, when a document is being written and `map`
/// has many entries.
pub(super) fn object(
    map: &Map<String, Value>,
    key: impl Fn(&str) -> ObjectKey,
    entry: impl Fn(&Value) -> Expression,
) -> Option<Expression> {
    if map.len() <= ENTRIES || !WRITING.with(|writing| writing.borrow().is_some()) {
        return None;
    }
    let mut text = String::from("{\n");
    let mut whole = false;
    for (name, value) in map {
        let mut one = Object::new();
        one.insert(key(name), entry(value));
        let written = hcl::format::to_string(&Expression::Object(one)).ok()?;
        whole |= written.contains("<<");
        let inner = written.strip_prefix("{\n")?.strip_suffix('}')?;
        text.push_str(inner);
    }
    text.push('}');
    WRITING.with(|writing| {
        let mut writing = writing.borrow_mut();
        let written = writing.as_mut()?;
        written.whole |= whole;
        let name = format!("config_editor_object_{}_", written.objects.len());
        written.objects.push((name.clone(), text));
        Some(Expression::Variable(Variable::unchecked(name)))
    })
}

/// `rendered` with each stand-in replaced by its object's text, its lines
/// indented as the stand-in's line is; `None` when a stand-in is not
/// written exactly once.
fn put_in_place(rendered: &str, objects: &[(String, String)]) -> Option<String> {
    let mut text = rendered.to_string();
    // Later objects stand inside earlier ones' texts only when written
    // inside them, so each is put in place once its holder is.
    for (name, object) in objects.iter().rev() {
        let mut found = text.match_indices(name.as_str());
        let (at, _) = found.next()?;
        if found.next().is_some() {
            return None;
        }
        let line_start = text[..at].rfind('\n').map_or(0, |newline| newline + 1);
        let indent: String = text[line_start..].chars().take_while(|character| *character == ' ').collect();
        let placed = object.replace('\n', &format!("\n{indent}"));
        // The last line (`}`) takes the indent; the entries' lines theirs.
        text.replace_range(at..at + name.len(), &placed);
    }
    Some(text)
}
