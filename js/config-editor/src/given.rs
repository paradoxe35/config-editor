//! What a list from JavaScript read last time, by document and key: a host
//! hands its whole list to `update` after every keystroke, its unchanged
//! documents' texts and schemas the same strings as before. Each one the
//! same (`Object.is`) as last time is taken from here instead of copied
//! into the module (and its schema compiled) again.

use std::collections::HashMap;

use js_sys::Object;
use wasm_bindgen::prelude::*;

/// A document's id and which of its keys.
type Key = (String, &'static str);

/// The values read from the last list, and those read from the list being
/// read (which replace them once it is read whole).
#[derive(Clone)]
pub(crate) struct Given<T> {
    last: HashMap<Key, (JsValue, T)>,
    read: HashMap<Key, (JsValue, T)>,
}

impl<T> Default for Given<T> {
    fn default() -> Self {
        Self {
            last: HashMap::new(),
            read: HashMap::new(),
        }
    }
}

impl<T: Clone> Given<T> {
    /// Document `id`'s `key`, given as `value`: what it read into last time
    /// when it is the same value, else `read` of it (which may look among
    /// the values already read, see [`Given::values`]).
    pub(crate) fn get(
        &mut self,
        id: &str,
        key: &'static str,
        value: &JsValue,
        read: impl FnOnce(&Self) -> Result<T, JsError>,
    ) -> Result<T, JsError> {
        let at = (id.to_string(), key);
        let kept = self
            .last
            .get(&at)
            .filter(|(known, _)| Object::is(known, value))
            .map(|(_, result)| result.clone());
        let result = match kept {
            Some(result) => result,
            None => read(self)?,
        };
        self.read.insert(at, (value.clone(), result.clone()));
        Ok(result)
    }

    /// Document `id`'s `key` is `value`, which reads into `read`: what the
    /// view handed the host (an edit's text), which it hands back.
    pub(crate) fn remember(&mut self, id: &str, key: &'static str, value: JsValue, read: T) {
        self.last.insert((id.to_string(), key), (value, read));
    }

    /// Document `id`'s `key` as the last list read it (or the view handed
    /// it), kept for this one: a key this list leaves out.
    pub(crate) fn carry(&mut self, id: &str, key: &'static str) -> Option<T> {
        let at = (id.to_string(), key);
        let last = self.last.get(&at)?.clone();
        let read = last.1.clone();
        self.read.insert(at, last);
        Some(read)
    }

    /// Document `id`'s `key` as the last list read it, or the view handed
    /// it since (what its editor holds, unless the host gave another).
    pub(crate) fn held(&self, id: &str, key: &'static str) -> Option<&T> {
        self.last
            .get(&(id.to_string(), key))
            .map(|(_, value)| value)
    }

    /// Every value read, from this list and the last.
    pub(crate) fn values(&self) -> impl Iterator<Item = &T> {
        self.read
            .values()
            .chain(self.last.values())
            .map(|(_, value)| value)
    }

    /// The list is read whole: what it read is kept for the next one.
    pub(crate) fn finish(&mut self) {
        self.last = std::mem::take(&mut self.read);
    }

    /// The list could not be read: the last one's values stay.
    pub(crate) fn abandon(&mut self) {
        self.read.clear();
    }
}
