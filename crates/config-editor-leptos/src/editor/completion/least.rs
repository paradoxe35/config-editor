//! The least list every schema applying to a list takes, as an inserted
//! block writes it. Their list rules are merged into one plan
//! ([`ListRules`]: `minItems` and `maxItems`, each position's item
//! schemas from `prefixItems` and `items`, each `contains` with its
//! `minContains` and `maxContains`, `uniqueItems`), and the plan answers
//! once whether some list meets them all, and which has the fewest items:
//! each item filled as a required value is, for every schema its position
//! has; the items matching a `contains` first, where the positions take
//! them, else after the positions that refuse them; past a `maxContains`
//! met, items that do not match; each distinct from the others under
//! `uniqueItems` where the item schemas allow another value
//! ([`others`]). So the list is no error the moment it is written.

use super::fill::{Filler, MAX_FILL_DEPTH, RESERVED_ITEMS};
use super::others::others;
use super::*;
use crate::editor::applies::{Contains, ListRules};

/// The item schema of a position no schema says anything of.
static ANYTHING: Value = Value::Bool(true);

/// The least list every schema at a list (`at`) takes, each item filled
/// as a required value is (a list in an item, a list of lists, the same
/// way); `None` when no list meets them that the language writes (an
/// item only `null` meets in TOML, more items asked than `items: false`
/// lets be). A list asking more items than the fill may still write gets
/// as many as it may, one at least; past [`MAX_FILL_DEPTH`] it is empty.
pub(super) fn least_items(
    filler: &mut Filler<'_>,
    at: &Entry<'_>,
    depth: usize,
) -> Option<Vec<Value>> {
    if depth >= MAX_FILL_DEPTH {
        return Some(Vec::new());
    }
    let here = Here::walk(at.clone(), filler.root, Seen::default());
    let rules = here.list_rules();
    if !rules.counts_fit() {
        return None;
    }
    let least = rules.least();
    // Past the bound, a list asking items gets one from the reserve, so a
    // language writing a list as repeated keys still writes it.
    let open = filler.items_left.saturating_sub(RESERVED_ITEMS);
    let budget = if open > 0 {
        open
    } else {
        filler.items_left.min(1)
    };
    let paid = least.min(budget);
    filler.items_left -= paid;
    // The search over positions is bounded: past the items `minItems`
    // asks, a position is added only for a `contains` match still asked
    // (at most the sum of their `minContains`) or past a tuple position
    // that refused one (at most the longest tuple). Past that bound a
    // longer list meets nothing a shorter one did not.
    let asked = rules
        .contains()
        .iter()
        .map(|contains| contains.least)
        .fold(0, usize::saturating_add);
    let longest = least.max(asked).saturating_add(rules.longest_tuple());
    let longest = rules.most().map_or(longest, |most| most.min(longest));
    let mut matched = vec![0; rules.contains().len()];
    let mut items: Vec<Value> = Vec::new();
    loop {
        let index = items.len();
        let wanted: Vec<usize> = (0..matched.len())
            .filter(|&rule| matched[rule] < rules.contains()[rule].least)
            .collect();
        if index >= least && wanted.is_empty() {
            return Some(items);
        }
        if index >= paid {
            // The fill's bound on items: as many as it may write.
            if index < least || filler.items_left == 0 {
                filler.cut = true;
                return Some(items);
            }
            filler.items_left -= 1;
        }
        if index >= longest {
            return None;
        }
        let item = Position {
            rules: &rules,
            index,
            items: &items,
        }
        .item(filler, &wanted, &matched, depth)?;
        for (rule, count) in matched.iter_mut().enumerate() {
            let decided = decides(rules.contains()[rule].schema, &item, filler.root);
            // An item written to match is counted unless it is known not to.
            if decided == Some(true) || (wanted.contains(&rule) && decided.is_none()) {
                *count += 1;
            }
        }
        items.push(item);
    }
}

/// Most item fills one fill tries again, for fewer `contains` matches
/// than a position's first try asked: past them each position takes its
/// first try only, so the work of a fill stays bounded however its lists
/// nest.
pub(super) const MAX_RETRIES: usize = 256;

/// One position of the least list being written.
struct Position<'p, 's> {
    rules: &'p ListRules<'s>,
    index: usize,
    /// The items before it.
    items: &'p [Value],
}

impl Position<'_, '_> {
    /// The item here, matching the `contains` still `wanted` where the
    /// position takes such an item: all of them, else one, else none of
    /// them; and matching no `contains` already `matched` as often as its
    /// `maxContains` allows. `None` when no item fits here.
    fn item(
        &self,
        filler: &mut Filler<'_>,
        wanted: &[usize],
        matched: &[usize],
        depth: usize,
    ) -> Option<Value> {
        let contains = self.rules.contains();
        let mut tries: Vec<Vec<usize>> = vec![wanted.to_vec()];
        if wanted.len() > 1 {
            tries.extend(wanted.iter().map(|&rule| vec![rule]));
        }
        if !wanted.is_empty() {
            tries.push(Vec::new());
        }
        for (count, matching) in tries.into_iter().enumerate() {
            if count > 0 {
                if filler.retries_left == 0 {
                    return None;
                }
                filler.retries_left -= 1;
            }
            let avoided: Vec<&Value> = (0..contains.len())
                .filter(|rule| !matching.contains(rule))
                .filter(|&rule| {
                    contains[rule]
                        .most
                        .is_some_and(|most| matched[rule] >= most)
                })
                .map(|rule| contains[rule].schema)
                .collect();
            let matching: Vec<&Contains<'_>> =
                matching.iter().map(|&rule| &contains[rule]).collect();
            if let Some(item) = self.filled(filler, &matching, &avoided, depth) {
                return Some(item);
            }
        }
        None
    }

    /// The first value filled for one of the schemas here and of
    /// `matching` that they all allow, that none of `avoided` takes, and
    /// that is distinct from the items before where they must be; else
    /// another value they allow so ([`others`]).
    fn filled(
        &self,
        filler: &mut Filler<'_>,
        matching: &[&Contains<'_>],
        avoided: &[&Value],
        depth: usize,
    ) -> Option<Value> {
        let mut schemas = self.rules.item_at(self.index);
        schemas.extend(matching.iter().map(|contains| contains.schema));
        if schemas.is_empty() {
            schemas.push(&ANYTHING);
        }
        let root = filler.root;
        let unique = self.rules.unique;
        let items = self.items;
        let allowed = |value: &Value| {
            schemas
                .iter()
                .all(|schema| decides(schema, value, root) != Some(false))
                && avoided
                    .iter()
                    .all(|schema| decides(schema, value, root) == Some(false))
                && !(unique && items.contains(value))
        };
        let every = Entry::all_of(&schemas);
        let mut filled = Vec::new();
        for schema in &schemas {
            if let Some(value) = filler.filled(schema, Some(&every), depth + 1) {
                // A list in it cut at the fill's bound is that bound,
                // not a refusal.
                if allowed(&value) || filler.cut {
                    return Some(value);
                }
                filled.push(value);
            }
        }
        let written = schemas[0];
        schemas
            .iter()
            .flat_map(|schema| others(schema, root, &filled, items.len() + 1))
            .find(|value| allowed(value) && filler.keeps(value, written))
    }
}
