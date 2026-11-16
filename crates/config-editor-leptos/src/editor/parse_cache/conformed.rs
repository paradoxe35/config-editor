//! The last documents read the way a schema expects (lone values where only
//! a list is allowed made lists, see `data_reading`), shared like the
//! parses they start from: one Edit-as save reads the host's text, the
//! text it writes and the one it saves in several of its steps and checks
//! (each patch reads its text back, then the save's last check does), and
//! walking a whole document through the schema costs as much as parsing
//! it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use serde_json::Value;

use super::super::data_reading::DataReading;
use super::super::language::{ConfigLanguage, ParseError};
use super::super::work_count::{Work, note_many};
use super::{HOLDERS, parse};

/// How many documents are kept: a save reads the host's last text, the
/// one it writes whole (to read its values back) and the one it patches,
/// and between HCL and XML the view's texts too.
const KEPT: usize = 3;

/// A document: the language's label, the text, the schema it was read
/// with (compared by value: a schema replaced may be allocated where the
/// one it replaces was) and the reading.
type Entry = (&'static str, String, Value, Rc<DataReading>);

thread_local! {
    /// The latest documents, the last read first.
    static LAST: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

/// `text` parsed as `language` and read as `schema` expects by `conform`,
/// from the last reading when it was of the same text in the same
/// language with the same schema.
pub(crate) fn conformed(
    language: &dyn ConfigLanguage,
    text: &str,
    schema: &Value,
    conform: impl FnOnce(Value) -> DataReading,
) -> Result<Rc<DataReading>, ParseError> {
    let label = language.info().label;
    let cached = LAST.with(|last| {
        let mut last = last.borrow_mut();
        let at = last.iter().position(|(known, written, read_with, _)| {
            *known == label && written == text && read_with == schema
        })?;
        let entry = last.remove(at);
        let reading = Rc::clone(&entry.3);
        last.insert(0, entry);
        Some(reading)
    });
    if let Some(reading) = cached {
        return Ok(reading);
    }
    let value = parse(language, text)?;
    note_many(Work::ConformedBytes, text.len());
    let reading = Rc::new(conform(value));
    if HOLDERS.with(Cell::get) > 0 {
        LAST.with(|last| {
            let mut last = last.borrow_mut();
            let entry = (label, text.to_string(), schema.clone(), Rc::clone(&reading));
            last.insert(0, entry);
            last.truncate(KEPT);
        });
    }
    Ok(reading)
}

/// Drops what is kept (the last editor unmounted).
pub(super) fn forget() {
    let _ = LAST.try_with(|last| last.borrow_mut().clear());
}
