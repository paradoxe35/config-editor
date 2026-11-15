//! A map an Edit-as save moves (cut and pasted in its view: `item_sources`
//! tells which, from the host's last text or an earlier one) is
//! written as the host wrote it where it was cut from, an object
//! attribute (`tags = { … }`) or a block, and a save that would switch it
//! waits (QA `E36-1`).

use super::convert::MapForms;
use crate::editor::item_sources::{self, ItemSource};
use crate::editor::language::{pointer_index, pointer_segments};
use std::collections::HashSet;

/// `pointer`'s path as map forms spell it: no list indices.
fn path(pointer: &str) -> Vec<String> {
    pointer_segments(pointer).into_iter().filter(|segment| pointer_index(segment).is_none()).collect()
}

/// `forms` (how `previous` writes its maps, read with `forms_of`) with
/// the forms of each item the save told moves (from the host's last
/// text, or the earlier one the links name) also at the path it is moved
/// to.
pub(super) fn with_moved(forms: MapForms, previous: &str, forms_of: impl Fn(&str) -> MapForms) -> MapForms {
    let Some(sources) = item_sources::told(previous) else {
        return forms;
    };
    let mut first = None;
    let (mut attributes, mut blocks) = forms.clone();
    for (pointer, source) in &sources.items {
        let (from, to) = (path(source.pointer()), path(pointer));
        let held = match source {
            ItemSource::Previous(_) if from == to => continue,
            ItemSource::Previous(_) => &forms,
            ItemSource::Origin(_) => first.get_or_insert_with(|| forms_of(&sources.origin)),
        };
        let at = |paths: &HashSet<Vec<String>>| -> Vec<Vec<String>> {
            paths
                .iter()
                .filter_map(|path| path.strip_prefix(from.as_slice()))
                .map(|rest| to.iter().chain(rest).cloned().collect())
                .collect()
        };
        let (moved_attributes, moved_blocks) = (at(&held.0), at(&held.1));
        for moved in &moved_attributes {
            blocks.remove(moved);
        }
        for moved in &moved_blocks {
            attributes.remove(moved);
        }
        attributes.extend(moved_attributes);
        blocks.extend(moved_blocks);
    }
    (attributes, blocks)
}
