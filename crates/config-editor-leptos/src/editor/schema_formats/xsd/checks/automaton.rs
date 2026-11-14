//! An XSD content model as an automaton over child elements: the children
//! of an element, in document order, are run through it, which checks
//! `xs:sequence` order, `xs:choice` (nested and repeated), substitution
//! groups and every `minOccurs`/`maxOccurs` at once, as an XSD validator
//! does. A model that is an `xs:all` is checked as a set (any order, each
//! at most as often as allowed).

use std::collections::BTreeSet;

use serde_json::{Value, json};

/// Copies a bounded `maxOccurs` (or `minOccurs`) is unrolled into; past
/// it, the particle repeats without bound (the count above is not
/// checked: a reported loss).
pub(in super::super) const MAX_UNROLLED: u64 = 64;

/// States an automaton may have; a larger model is not checked (a
/// reported loss), so a check stays linear in the document.
pub(in super::super) const MAX_STATES: usize = 20_000;

/// The namespaces an `xs:any` or `xs:anyAttribute` allows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) enum Namespaces {
    /// `##any`.
    Any,
    /// `##other`: a namespace, not this one (nor none).
    Not(Option<String>),
    /// A list: these namespaces (`None`: `##local`, no namespace).
    In(Vec<Option<String>>),
}

impl Namespaces {
    pub(in super::super) fn to_json(&self) -> Value {
        match self {
            Namespaces::Any => json!("##any"),
            Namespaces::Not(namespace) => json!({ "not": namespace }),
            Namespaces::In(namespaces) => json!({ "in": namespaces }),
        }
    }

    pub(in super::super) fn from_json(value: &Value) -> Option<Namespaces> {
        if value.as_str() == Some("##any") {
            return Some(Namespaces::Any);
        }
        if let Some(namespace) = value.get("not") {
            return Some(Namespaces::Not(namespace.as_str().map(str::to_string)));
        }
        let list = value.get("in")?.as_array()?;
        Some(Namespaces::In(
            list.iter()
                .map(|namespace| namespace.as_str().map(str::to_string))
                .collect(),
        ))
    }

    /// Whether a name in `namespace` is allowed.
    pub(in super::super) fn allows(&self, namespace: &Namespace) -> bool {
        let Namespace::Known(namespace) = namespace else {
            return true;
        };
        match self {
            Namespaces::Any => true,
            Namespaces::Not(not) => namespace.is_some() && namespace != not,
            Namespaces::In(list) => list.contains(namespace),
        }
    }
}

/// A child's namespace: known from an XML text (`None`: no namespace), or
/// unknown (another language's text), which any wildcard allows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) enum Namespace {
    Known(Option<String>),
    Unknown,
}

/// A child element as the automaton reads it.
#[derive(Clone, Debug)]
pub(in super::super) struct Child<'a> {
    pub(in super::super) name: &'a str,
    pub(in super::super) namespace: Namespace,
}

/// A content model particle, as the compiler writes it (`x-xsd-content`).
#[derive(Clone, Debug, PartialEq)]
pub(in super::super) enum Particle {
    Element(String),
    /// `xs:any`.
    Any(Namespaces),
    Sequence(Vec<Occurs>),
    Choice(Vec<Occurs>),
    All(Vec<Occurs>),
}

/// A particle with its `minOccurs` and `maxOccurs` (`None`: unbounded).
#[derive(Clone, Debug, PartialEq)]
pub(in super::super) struct Occurs {
    pub(in super::super) particle: Particle,
    pub(in super::super) min: u64,
    pub(in super::super) max: Option<u64>,
}

impl Occurs {
    /// `particle` once.
    pub(in super::super) fn once(particle: Particle) -> Occurs {
        Occurs {
            particle,
            min: 1,
            max: Some(1),
        }
    }

    /// The model as JSON (what `x-xsd-content` holds).
    pub(in super::super) fn to_json(&self) -> Value {
        let (kind, inner) = match &self.particle {
            Particle::Element(name) => {
                return json!({"element": name, "min": self.min, "max": self.max});
            }
            Particle::Any(namespaces) => {
                return json!({"any": namespaces.to_json(), "min": self.min, "max": self.max});
            }
            Particle::Sequence(items) => ("sequence", items),
            Particle::Choice(items) => ("choice", items),
            Particle::All(items) => ("all", items),
        };
        let items: Vec<Value> = inner.iter().map(Occurs::to_json).collect();
        json!({ kind: items, "min": self.min, "max": self.max })
    }

    /// The model `value` (written by [`to_json`](Self::to_json)) holds.
    pub(in super::super) fn from_json(value: &Value) -> Option<Occurs> {
        let min = value.get("min")?.as_u64()?;
        let max = match value.get("max")? {
            Value::Null => None,
            other => Some(other.as_u64()?),
        };
        let items = |key: &str| -> Option<Vec<Occurs>> {
            value
                .get(key)?
                .as_array()?
                .iter()
                .map(Occurs::from_json)
                .collect()
        };
        let particle = if let Some(name) = value.get("element").and_then(Value::as_str) {
            Particle::Element(name.to_string())
        } else if let Some(namespaces) = value.get("any") {
            Particle::Any(Namespaces::from_json(namespaces)?)
        } else if let Some(items) = items("sequence") {
            Particle::Sequence(items)
        } else if let Some(items) = items("choice") {
            Particle::Choice(items)
        } else {
            Particle::All(items("all")?)
        };
        Some(Occurs { particle, min, max })
    }

    /// The element names the model names.
    pub(in super::super) fn names(&self, into: &mut BTreeSet<String>) {
        match &self.particle {
            Particle::Element(name) => {
                into.insert(name.clone());
            }
            Particle::Any(_) => {}
            Particle::Sequence(items) | Particle::Choice(items) | Particle::All(items) => {
                items.iter().for_each(|item| item.names(into));
            }
        }
    }

    /// The element names it names, in the order it first names them.
    pub(in super::super) fn ordered_names(&self, into: &mut Vec<String>) {
        match &self.particle {
            Particle::Element(name) => {
                if !into.contains(name) {
                    into.push(name.clone());
                }
            }
            Particle::Any(_) => {}
            Particle::Sequence(items) | Particle::Choice(items) | Particle::All(items) => {
                items.iter().for_each(|item| item.ordered_names(into));
            }
        }
    }

    /// Whether a sequence in it orders two elements.
    pub(in super::super) fn orders(&self) -> bool {
        match &self.particle {
            Particle::Sequence(items) => {
                let mut names = Vec::new();
                items.iter().for_each(|item| item.ordered_names(&mut names));
                names.len() > 1 || items.iter().any(Occurs::orders)
            }
            Particle::Choice(items) | Particle::All(items) => items.iter().any(Occurs::orders),
            _ => false,
        }
    }

    /// The namespaces of its `xs:any`s.
    pub(in super::super) fn wildcards(&self, into: &mut Vec<Namespaces>) {
        match &self.particle {
            Particle::Element(_) => {}
            Particle::Any(namespaces) => into.push(namespaces.clone()),
            Particle::Sequence(items) | Particle::Choice(items) | Particle::All(items) => {
                items.iter().for_each(|item| item.wildcards(into));
            }
        }
    }

    /// Whether it has an `xs:any`.
    pub(in super::super) fn has_wildcard(&self) -> bool {
        match &self.particle {
            Particle::Element(_) => false,
            Particle::Any(_) => true,
            Particle::Sequence(items) | Particle::Choice(items) | Particle::All(items) => {
                items.iter().any(Occurs::has_wildcard)
            }
        }
    }

    /// Whether a count above [`MAX_UNROLLED`] is in it (not checked).
    pub(in super::super) fn counts_past_unrolled(&self) -> bool {
        self.min > MAX_UNROLLED
            || self.max.is_some_and(|max| max > MAX_UNROLLED)
            || match &self.particle {
                Particle::Sequence(items) | Particle::Choice(items) | Particle::All(items) => {
                    items.iter().any(Occurs::counts_past_unrolled)
                }
                _ => false,
            }
    }
}

/// What a run of children found wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) enum Mismatch {
    /// Child `index` is not allowed where it is; `expected` what would be
    /// (empty: nothing more is allowed).
    Unexpected { index: usize, expected: Vec<String> },
    /// The children end before the model does; `expected` what must come.
    Missing { expected: Vec<String> },
}

/// A Thompson automaton: each state either matches one child and moves
/// on, or moves on without matching (to any of its targets).
pub(in super::super) enum State {
    Symbol(Symbol, usize),
    Split(Vec<usize>),
    Accept,
}

pub(in super::super) enum Symbol {
    Name(String),
    Any(Namespaces),
}

/// A content model ready to run (built once per model and check).
pub(in super::super) enum Automaton {
    States { states: Vec<State>, start: usize },
    All { items: Vec<Occurs>, min: u64 },
}

struct Builder {
    states: Vec<State>,
}

impl Builder {
    fn push(&mut self, state: State) -> Option<usize> {
        if self.states.len() >= MAX_STATES {
            return None;
        }
        self.states.push(state);
        Some(self.states.len() - 1)
    }

    /// States matching `occurs` and then going to `next`; the start.
    fn build(&mut self, occurs: &Occurs, next: usize) -> Option<usize> {
        let min = occurs.min.min(MAX_UNROLLED);
        let mut start = match occurs.max {
            Some(max) if max <= MAX_UNROLLED => {
                // The optional copies, from the back.
                let mut at = next;
                for _ in min..max.max(min) {
                    let body = self.once(&occurs.particle, at)?;
                    at = self.push(State::Split(vec![body, next]))?;
                }
                at
            }
            // Unbounded (or past the unrolled count): a loop.
            _ => {
                let split = self.push(State::Split(Vec::new()))?;
                let body = self.once(&occurs.particle, split)?;
                if let State::Split(targets) = &mut self.states[split] {
                    *targets = vec![body, next];
                }
                split
            }
        };
        for _ in 0..min {
            start = self.once(&occurs.particle, start)?;
        }
        Some(start)
    }

    /// States matching `particle` once and then going to `next`.
    fn once(&mut self, particle: &Particle, next: usize) -> Option<usize> {
        match particle {
            Particle::Element(name) => self.push(State::Symbol(Symbol::Name(name.clone()), next)),
            Particle::Any(namespaces) => {
                self.push(State::Symbol(Symbol::Any(namespaces.clone()), next))
            }
            Particle::Sequence(items) => {
                let mut at = next;
                for item in items.iter().rev() {
                    at = self.build(item, at)?;
                }
                Some(at)
            }
            Particle::Choice(items) => {
                let starts = items
                    .iter()
                    .map(|item| self.build(item, next))
                    .collect::<Option<Vec<usize>>>()?;
                self.push(State::Split(starts))
            }
            // An `xs:all` inside another model (an extension of a base
            // with content): its items in any order, any number of times
            // (the compiler reports the loss).
            Particle::All(items) => {
                let split = self.push(State::Split(Vec::new()))?;
                let mut starts = items
                    .iter()
                    .map(|item| self.once(&item.particle, split))
                    .collect::<Option<Vec<usize>>>()?;
                starts.push(next);
                if let State::Split(targets) = &mut self.states[split] {
                    *targets = starts;
                }
                Some(split)
            }
        }
    }
}

impl Automaton {
    /// `model` ready to run; `None` past [`MAX_STATES`].
    pub(in super::super) fn new(model: &Occurs) -> Option<Automaton> {
        if let Particle::All(items) = &model.particle {
            return Some(Automaton::All {
                items: items.clone(),
                min: model.min,
            });
        }
        let mut builder = Builder { states: Vec::new() };
        let accept = builder.push(State::Accept)?;
        let start = builder.build(model, accept)?;
        Some(Automaton::States {
            states: builder.states,
            start,
        })
    }

    /// Runs `children` (in document order) through it.
    pub(in super::super) fn run(&self, children: &[Child<'_>]) -> Result<(), Mismatch> {
        match self {
            Automaton::All { items, min } => run_all(items, *min, children),
            Automaton::States { states, start } => run_states(states, *start, children),
        }
    }
}

fn run_states(states: &[State], start: usize, children: &[Child<'_>]) -> Result<(), Mismatch> {
    let mut seen = vec![usize::MAX; states.len()];
    let mut round = 0;
    let mut closure = |from: Vec<usize>, round: usize| -> Vec<usize> {
        let mut stack = from;
        let mut out = Vec::new();
        while let Some(at) = stack.pop() {
            if std::mem::replace(&mut seen[at], round) == round {
                continue;
            }
            match &states[at] {
                State::Split(targets) => stack.extend(targets.iter().copied()),
                _ => out.push(at),
            }
        }
        out
    };
    let expected = |current: &[usize]| -> Vec<String> {
        let mut found: BTreeSet<String> = BTreeSet::new();
        for at in current {
            match &states[*at] {
                State::Symbol(Symbol::Name(name), _) => {
                    found.insert(format!("<{name}>"));
                }
                State::Symbol(Symbol::Any(_), _) => {
                    found.insert("another element".to_string());
                }
                _ => {}
            }
        }
        found.into_iter().collect()
    };
    let mut current = closure(vec![start], round);
    for (index, child) in children.iter().enumerate() {
        let next: Vec<usize> = current
            .iter()
            .filter_map(|at| match &states[*at] {
                State::Symbol(Symbol::Name(name), to) if name == child.name => Some(*to),
                State::Symbol(Symbol::Any(namespaces), to)
                    if namespaces.allows(&child.namespace) =>
                {
                    Some(*to)
                }
                _ => None,
            })
            .collect();
        if next.is_empty() {
            return Err(Mismatch::Unexpected {
                index,
                expected: expected(&current),
            });
        }
        round += 1;
        current = closure(next, round);
    }
    if current
        .iter()
        .any(|at| matches!(states[*at], State::Accept))
    {
        Ok(())
    } else {
        Err(Mismatch::Missing {
            expected: expected(&current),
        })
    }
}

/// An `xs:all`: each child at most as often as its `maxOccurs`, the
/// required ones present, in any order.
fn run_all(items: &[Occurs], min: u64, children: &[Child<'_>]) -> Result<(), Mismatch> {
    let mut counts = vec![0_u64; items.len()];
    let named = |item: &Occurs| match &item.particle {
        Particle::Element(name) => Some(format!("<{name}>")),
        _ => None,
    };
    for (index, child) in children.iter().enumerate() {
        let at = items.iter().position(
            |item| matches!(&item.particle, Particle::Element(name) if name == child.name),
        );
        match at {
            Some(at) if items[at].max.is_none_or(|max| counts[at] < max) => counts[at] += 1,
            _ => {
                let expected = items
                    .iter()
                    .zip(&counts)
                    .filter(|(item, count)| item.max.is_none_or(|max| **count < max))
                    .filter_map(|(item, _)| named(item))
                    .collect();
                return Err(Mismatch::Unexpected { index, expected });
            }
        }
    }
    if children.is_empty() && min == 0 {
        return Ok(());
    }
    let missing: Vec<String> = items
        .iter()
        .zip(&counts)
        .filter(|(item, count)| **count < item.min)
        .filter_map(|(item, _)| named(item))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(Mismatch::Missing { expected: missing })
    }
}

#[cfg(test)]
mod tests {
    use super::{Automaton, Child, Mismatch, Namespace, Namespaces, Occurs, Particle};

    fn element(name: &str, min: u64, max: Option<u64>) -> Occurs {
        Occurs {
            particle: Particle::Element(name.to_string()),
            min,
            max,
        }
    }

    fn group(particle: Particle, min: u64, max: Option<u64>) -> Occurs {
        Occurs { particle, min, max }
    }

    fn run(model: &Occurs, names: &[&str]) -> Result<(), Mismatch> {
        let children: Vec<Child<'_>> = names
            .iter()
            .map(|name| Child {
                name,
                namespace: Namespace::Unknown,
            })
            .collect();
        Automaton::new(model).unwrap().run(&children)
    }

    #[test]
    fn a_content_model_accepts_what_xsd_accepts() {
        // (a*, (b | c)+, d?)
        let model = group(
            Particle::Sequence(vec![
                element("a", 0, None),
                group(
                    Particle::Choice(vec![element("b", 1, Some(1)), element("c", 1, Some(1))]),
                    1,
                    None,
                ),
                element("d", 0, Some(1)),
            ]),
            1,
            Some(1),
        );
        let cases: &[(&[&str], bool)] = &[
            (&["b"], true),
            (&["a", "a", "c", "b", "d"], true),
            (&["a", "b", "a"], false),
            (&["d"], false),
            (&["b", "d", "d"], false),
            (&[], false),
            (&["x"], false),
        ];
        for (children, valid) in cases {
            assert_eq!(run(&model, children).is_ok(), *valid, "{children:?}");
        }
        assert_eq!(
            run(&model, &["a", "b", "a"]),
            Err(Mismatch::Unexpected {
                index: 2,
                expected: vec!["<b>".into(), "<c>".into(), "<d>".into()]
            })
        );
        assert_eq!(
            run(&model, &["a"]),
            Err(Mismatch::Missing {
                expected: vec!["<a>".into(), "<b>".into(), "<c>".into()]
            })
        );
        // Bounded counts.
        let two = group(
            Particle::Sequence(vec![element("i", 1, Some(2))]),
            1,
            Some(1),
        );
        assert!(run(&two, &["i", "i"]).is_ok());
        assert!(run(&two, &["i", "i", "i"]).is_err());
        // A choice of one option, not several.
        let one_of = group(
            Particle::Choice(vec![element("x", 1, Some(1)), element("y", 0, Some(1))]),
            1,
            Some(1),
        );
        assert!(run(&one_of, &["x"]).is_ok());
        assert!(run(&one_of, &[]).is_ok());
        assert!(run(&one_of, &["x", "y"]).is_err());
        // xs:all: any order, each once.
        let all = group(
            Particle::All(vec![element("x", 1, Some(1)), element("y", 0, Some(1))]),
            1,
            Some(1),
        );
        assert!(run(&all, &["y", "x"]).is_ok());
        assert!(run(&all, &["x", "x"]).is_err());
        assert!(run(&all, &["y"]).is_err());
        // The JSON form round-trips.
        assert_eq!(Occurs::from_json(&model.to_json()), Some(model));
    }

    #[test]
    fn a_wildcard_takes_the_namespaces_it_allows() {
        let any = |namespaces: Namespaces| {
            group(
                Particle::Sequence(vec![
                    element("a", 1, Some(1)),
                    group(Particle::Any(namespaces), 0, None),
                ]),
                1,
                Some(1),
            )
        };
        let other = any(Namespaces::Not(Some("urn:t".into())));
        let child = |name, namespace: Option<&str>| Child {
            name,
            namespace: Namespace::Known(namespace.map(str::to_string)),
        };
        let automaton = Automaton::new(&other).unwrap();
        assert!(
            automaton
                .run(&[child("a", Some("urn:t")), child("z", Some("urn:o"))])
                .is_ok()
        );
        assert!(
            automaton
                .run(&[child("a", Some("urn:t")), child("z", Some("urn:t"))])
                .is_err()
        );
        assert!(
            automaton
                .run(&[child("a", Some("urn:t")), child("z", None)])
                .is_err()
        );
        // Another language's text: no namespace known, any is allowed.
        assert!(run(&other, &["a", "z"]).is_ok());
        let local = Automaton::new(&any(Namespaces::In(vec![None]))).unwrap();
        assert!(local.run(&[child("a", None), child("z", None)]).is_ok());
        assert!(
            local
                .run(&[child("a", None), child("z", Some("urn:o"))])
                .is_err()
        );
        // xs:any (##any) takes a named element too, after the one named.
        assert!(run(&any(Namespaces::Any), &["a", "a"]).is_ok());
        assert_eq!(Occurs::from_json(&other.to_json()), Some(other));
    }

    #[test]
    fn a_model_too_large_is_not_built() {
        // 64 copies of 64 copies of 64 copies.
        let inner = element("x", 64, Some(64));
        let middle = group(Particle::Sequence(vec![inner]), 64, Some(64));
        let outer = group(Particle::Sequence(vec![middle]), 64, Some(64));
        assert!(Automaton::new(&outer).is_none());
    }
}
