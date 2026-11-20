//! What every schema applying to one list says of it, merged: how many
//! items it may have (`minItems`, `maxItems`, a closed tuple), the
//! schemas each position's item meets (`prefixItems`, `items`, draft-07's
//! `items` list and `additionalItems`), how many items match each
//! `contains` (`minContains`, `maxContains`), and `uniqueItems`. One
//! reading for every question about the list as a whole (whether some
//! list is taken, the least list a fill writes), so they agree on every
//! way the rules are split between schemas.

use serde_json::Value;

/// The list rules of a set of schemas applying to one value together.
#[derive(Debug, Default)]
pub(crate) struct ListRules<'s> {
    /// The schemas that give item positions (`prefixItems`, `items`).
    tuples: Vec<&'s Value>,
    /// The most `minItems` asks.
    least: usize,
    /// The fewest items any rule allows: `maxItems`, a closed tuple's
    /// length, the first position refused (`false`).
    most: Option<usize>,
    /// Each `contains` with its bounds.
    contains: Vec<Contains<'s>>,
    /// Whether one asks `uniqueItems`.
    pub(crate) unique: bool,
}

/// One `contains` and how many items must match it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Contains<'s> {
    pub(crate) schema: &'s Value,
    /// `minContains` (one without it).
    pub(crate) least: usize,
    /// `maxContains`.
    pub(crate) most: Option<usize>,
}

impl<'s> ListRules<'s> {
    /// The rules `schemas` (each applying to the list) give, merged.
    pub(crate) fn of(schemas: &[&'s Value]) -> Self {
        let mut rules = Self::default();
        for schema in schemas {
            rules.add(schema);
        }
        rules
    }

    fn add(&mut self, schema: &'s Value) {
        let Some(members) = schema.as_object() else {
            return;
        };
        let size = |keyword: &str| {
            members
                .get(keyword)
                .and_then(Value::as_u64)
                .map(|size| usize::try_from(size).unwrap_or(usize::MAX))
        };
        if let Some(least) = size("minItems") {
            self.least = self.least.max(least);
        }
        self.bound(size("maxItems"));
        if let Some(contains) = members.get("contains") {
            self.contains.push(Contains {
                schema: contains,
                least: size("minContains").unwrap_or(1),
                most: size("maxContains"),
            });
        }
        self.unique |= members.get("uniqueItems") == Some(&Value::Bool(true));
        let (positions, rest) = positions(schema);
        if positions.is_some() || rest.is_some() {
            self.tuples.push(schema);
        }
        let positions = positions.unwrap_or_default();
        let refused = positions
            .iter()
            .position(|item| *item == Value::Bool(false));
        self.bound(refused);
        if rest == Some(&Value::Bool(false)) {
            self.bound(Some(positions.len()));
        }
    }

    fn bound(&mut self, most: Option<usize>) {
        if let Some(most) = most {
            self.most = Some(self.most.map_or(most, |known| known.min(most)));
        }
    }

    /// Whether the rules say anything of a list.
    pub(crate) fn is_empty(&self) -> bool {
        self.tuples.is_empty()
            && self.least == 0
            && self.most.is_none()
            && self.contains.is_empty()
            && !self.unique
    }

    /// The fewest items the rules ask: `minItems`, and each `contains`'s
    /// `minContains` (the matches of two may be the same items).
    pub(crate) fn least(&self) -> usize {
        self.contains
            .iter()
            .map(|contains| contains.least)
            .fold(self.least, usize::max)
    }

    /// The most items the rules allow.
    pub(crate) fn most(&self) -> Option<usize> {
        self.most
    }

    /// Whether the counts the rules ask can all be met at once: no more
    /// items asked than allowed, no `contains` asking more matches than it
    /// allows. (Whether items meeting them exist is the fill's question.)
    pub(crate) fn counts_fit(&self) -> bool {
        let fits = |least: usize, most: Option<usize>| most.is_none_or(|most| least <= most);
        fits(self.least(), self.most)
            && self
                .contains
                .iter()
                .all(|contains| fits(contains.least, contains.most))
    }

    pub(crate) fn contains(&self) -> &[Contains<'s>] {
        &self.contains
    }

    /// The longest tuple the rules give positions for.
    pub(crate) fn longest_tuple(&self) -> usize {
        self.tuples
            .iter()
            .map(|schema| positions(schema).0.map_or(0, <[Value]>::len))
            .max()
            .unwrap_or(0)
    }

    /// The schemas the item at `index` meets, one from each schema giving
    /// positions; none says nothing of it. A `true` or `{}` says nothing,
    /// a `false` is past [`Self::most`].
    pub(crate) fn item_at(&self, index: usize) -> Vec<&'s Value> {
        self.tuples
            .iter()
            .filter_map(|schema| {
                let (positions, rest) = positions(schema);
                positions
                    .and_then(|positions| positions.get(index))
                    .or(rest)
            })
            .filter(|item| !says_nothing(item))
            .collect()
    }
}

/// A schema's item positions (`prefixItems`, or up to draft-07 an `items`
/// list) and the schema of the items after them (`items`, or after a
/// draft-07 list `additionalItems`).
fn positions(schema: &Value) -> (Option<&[Value]>, Option<&Value>) {
    let tuple = |keyword: &str| schema.get(keyword).and_then(Value::as_array);
    let rest = |keyword: &str| schema.get(keyword).filter(|rest| !rest.is_array());
    if let Some(positions) = tuple("prefixItems") {
        return (Some(positions), rest("items"));
    }
    if let Some(positions) = tuple("items") {
        return (Some(positions), rest("additionalItems"));
    }
    (None, rest("items"))
}

/// Whether an item schema allows every value (`true`, `{}`).
fn says_nothing(schema: &Value) -> bool {
    match schema {
        Value::Bool(allows) => *allows,
        Value::Object(members) => members.is_empty(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rules_split_between_schemas_merge_into_one() {
        let typed = json!({"type": "array", "prefixItems": [{"type": "null"}]});
        let asked = json!({"minItems": 2, "contains": {"type": "integer"}, "maxItems": 3});
        let closed = json!({"items": false, "prefixItems": [{}, {"minimum": 1}, {}, {}]});
        let rules = ListRules::of(&[&typed, &asked, &closed]);
        assert_eq!((rules.least(), rules.most()), (2, Some(3)));
        assert_eq!(rules.item_at(0), vec![&json!({"type": "null"})]);
        assert_eq!(rules.item_at(1), vec![&json!({"minimum": 1})]);
        assert!(rules.item_at(2).is_empty());
        assert_eq!(rules.longest_tuple(), 4);
        assert!(rules.counts_fit());
        let none = json!({"minItems": 1, "items": false});
        assert_eq!(ListRules::of(&[&none]).most(), Some(0));
        assert!(!ListRules::of(&[&none]).counts_fit());
        let draft7 = json!({"items": [{"type": "string"}], "additionalItems": false});
        assert_eq!(ListRules::of(&[&draft7]).most(), Some(1));
        let refused = json!({"prefixItems": [{}, false]});
        assert_eq!(ListRules::of(&[&refused]).most(), Some(1));
        let few = json!({"contains": {}, "minContains": 3, "maxContains": 2});
        assert!(!ListRules::of(&[&few]).counts_fit());
    }
}
