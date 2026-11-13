//! The document path of each element, planned before any span is recorded
//! so repeated siblings are numbered as `build_value` folds them into
//! arrays, and so the config root can be skipped. Planning reads each
//! element once (its name, parent and place among same-named siblings);
//! a path is built when it is asked for, from its ancestors', so the
//! caret's path on a keystroke costs its depth, not the document's every
//! path.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};

use super::*;

/// Elements deeper than this get no path (the parser refuses past 128).
const MAX_PLANNED_DEPTH: usize = 256;

/// Document paths of every element, keyed by the byte offset of its start
/// tag. The config root's path is empty, so every recorded span is already
/// relative to the schema root.
pub(super) struct PathPlan {
    nodes: Vec<PlannedNode>,
    /// Each element's path, built on first use.
    paths: Vec<OnceCell<Vec<Seg>>>,
    /// The config root (`<any config-root="true">`), if any.
    wrapper: Option<usize>,
    /// Start of the config root's start tag, if the document has one. Its
    /// attributes are editor metadata.
    pub(super) root: Option<usize>,
    /// Each element's children's names, by the element's id.
    child_names: HashSet<(usize, String)>,
    /// Whether no two elements' paths read alike: no element named like an
    /// index (`<0>`), and no element beside the config root at the top
    /// (whose children's paths would start like its own).
    unique: bool,
}

/// An element seen while planning: enough structure to find the config root
/// and to number repeated siblings.
struct PlannedNode {
    name: String,
    start: usize,
    parent: Option<usize>,
    depth: usize,
    config_root: bool,
    /// Its place among its parent's children of its name.
    occurrence: usize,
    /// Whether its parent has more than one child of its name.
    repeated: bool,
}

impl PathPlan {
    /// The path of the element whose start tag starts at `start`.
    pub(super) fn path_of(&self, start: usize) -> Option<&[Seg]> {
        // In document order: found by halving, not by a walk per lookup.
        let id = self.nodes.binary_search_by_key(&start, |node| node.start).ok()?;
        // Its ancestors' paths first, top down, each from its parent's.
        let mut missing = Vec::new();
        let mut at = Some(id);
        while let Some(node) = at.filter(|node| self.paths[*node].get().is_none()) {
            missing.push(node);
            at = self.nodes[node].parent;
        }
        for node in missing.into_iter().rev() {
            let _ = self.paths[node].set(self.build(node));
        }
        self.paths[id].get().map(Vec::as_slice)
    }

    /// The key of the attribute `name` of the element whose start tag
    /// starts at `start`: `@name` when a child element has its name too
    /// (`<r a="1"><a>2</a></r>`), so the two stay apart; else `name`.
    pub(super) fn attribute_key(&self, start: usize, name: &str) -> String {
        let id = self.nodes.binary_search_by_key(&start, |node| node.start).ok();
        match id {
            Some(id) if self.child_names.contains(&(id, name.to_string())) => format!("@{name}"),
            _ => name.to_string(),
        }
    }

    /// The element whose path reads `path` among those starting at
    /// `starts`, and the start of the first element after it and all it
    /// holds (`None` at the document's end): every path starting with
    /// `path` is its or one inside it, so its keys are read there alone.
    /// `None` where paths may read alike, or for the empty path (every
    /// element's).
    pub(super) fn subtree(
        &self,
        path: &[String],
        starts: impl IntoIterator<Item = usize>,
    ) -> Option<(usize, Option<usize>)> {
        if !self.unique || path.is_empty() {
            return None;
        }
        let start = starts
            .into_iter()
            .find(|start| self.path_of(*start).is_some_and(|found| strings(found) == path))?;
        let id = self.nodes.binary_search_by_key(&start, |node| node.start).ok()?;
        // Planned from a stack: what an element holds follows it, deeper.
        let depth = self.nodes[id].depth;
        let after = self.nodes[id + 1..].iter().find(|node| node.depth <= depth);
        Some((start, after.map(|node| node.start)))
    }

    /// The path of element `id`, its parent's being built.
    fn build(&self, id: usize) -> Vec<Seg> {
        crate::editor::work_count::note(crate::editor::work_count::Work::ElementPath);
        let node = &self.nodes[id];
        // The config root's path is empty; deeper than the parser reads (a
        // syntax error anyway), none.
        if Some(id) == self.wrapper || node.depth > MAX_PLANNED_DEPTH {
            return Vec::new();
        }
        let mut path = node
            .parent
            .and_then(|parent| self.paths[parent].get())
            .cloned()
            .unwrap_or_default();
        path.push(Seg::Key(node.name.clone()));
        if node.repeated {
            path.push(Seg::Index(node.occurrence));
        }
        path
    }
}

thread_local! {
    static LAST_PLAN: std::cell::RefCell<Option<(u64, usize, std::rc::Rc<PathPlan>)>> = const { std::cell::RefCell::new(None) };
}

/// Forgets the last text's plan.
pub(super) fn forget_plan() {
    let _ = LAST_PLAN.try_with(|last| last.borrow_mut().take());
}

#[cfg(test)]
pub(super) fn plan_kept() -> bool {
    LAST_PLAN.with(|last| last.borrow().is_some())
}

/// [`plan_text`] of `source` (whose tokens are `tokens`), remembered for
/// the last text.
pub(super) fn plan_paths(source: &str, tokens: &[Token]) -> std::rc::Rc<PathPlan> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let key = (hasher.finish(), source.len());
    if let Some(plan) = LAST_PLAN.with(|last| {
        last.borrow().as_ref().filter(|(hash, len, _)| (*hash, *len) == key).map(|(_, _, plan)| plan.clone())
    }) {
        return plan;
    }
    let plan = std::rc::Rc::new(plan_text(source, tokens));
    LAST_PLAN.with(|last| *last.borrow_mut() = Some((key.0, key.1, plan.clone())));
    plan
}

pub(super) fn plan_text(source: &str, tokens: &[Token]) -> PathPlan {
    let mut nodes: Vec<PlannedNode> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for token in tokens {
        match token.kind {
            Kind::OpenTag { .. } => {
                let name = token_name(source, token);
                if name.is_empty() {
                    continue;
                }
                let self_closing = opens_nothing(source, token);
                let id = nodes.len();
                let parent = stack.last().copied();
                nodes.push(PlannedNode {
                    name,
                    start: token.start,
                    parent,
                    depth: parent.map_or(0, |parent| nodes[parent].depth + 1),
                    config_root: stack.is_empty() && marks_config_root(source, token),
                    occurrence: 0,
                    repeated: false,
                });
                if !self_closing {
                    stack.push(id);
                }
            }
            Kind::EndTag => {
                let name = token_name(source, token);
                if stack.last().is_some_and(|id| nodes[*id].name == name) {
                    stack.pop();
                } else if let Some(position) = stack.iter().rposition(|id| nodes[*id].name == name)
                {
                    stack.truncate(position);
                } else {
                    // An end tag matching no open element (`<listen>1</listn>`):
                    // a misspelled end of the innermost one, which it closes,
                    // so the elements after it are not all nested inside it.
                    stack.pop();
                }
            }
            _ => {}
        }
    }

    // Several marked roots are a parse error; the tolerant scan keeps the first.
    let wrapper = nodes.iter().position(|node| node.config_root);

    // Each element's place among its same-named siblings, and how many
    // there are (the config root is no one's sibling).
    let mut counts: HashMap<(Option<usize>, &str), usize> = HashMap::new();
    let mut occurrences = vec![0; nodes.len()];
    for (id, node) in nodes.iter().enumerate() {
        if Some(id) == wrapper {
            continue;
        }
        let count = counts.entry((node.parent, node.name.as_str())).or_insert(0);
        occurrences[id] = *count;
        *count += 1;
    }
    let repeated: Vec<bool> = nodes
        .iter()
        .map(|node| counts.get(&(node.parent, node.name.as_str())).copied().unwrap_or(0) > 1)
        .collect();
    for ((node, occurrence), repeated) in nodes.iter_mut().zip(occurrences).zip(repeated) {
        node.occurrence = occurrence;
        node.repeated = repeated;
    }
    let child_names = nodes
        .iter()
        .filter_map(|node| Some((node.parent?, node.name.clone())))
        .collect();
    let unique = nodes.iter().all(|node| {
        !node.name.bytes().all(|byte| byte.is_ascii_digit())
            && (wrapper.is_none() || node.parent.is_some() || Some(node.start) == wrapper.map(|id| nodes[id].start))
    });
    PathPlan {
        unique,
        child_names,
        paths: (0..nodes.len()).map(|_| OnceCell::new()).collect(),
        root: wrapper.map(|id| nodes[id].start),
        wrapper,
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use crate::Language;
    use crate::editor::work_count::{Work, counted};

    /// QA 22: every keystroke in a long XML document built the path of
    /// each of its elements (each a copy of its parent's) for the caret's
    /// suggestions, a long task at 6000 lines. Only the caret's element
    /// and its ancestors are built.
    #[test]
    fn the_caret_builds_only_its_own_elements_path() {
        let mut text = String::from("<root>\n");
        for index in 0..1500 {
            text += &format!("  <svc_{index}>\n    <host>h{index}</host>\n    <port>{index}</port>\n  </svc_{index}>\n");
        }
        text += "</root>\n";
        let behavior = Language::XML.behavior();
        let at = text.find("h750</host>").unwrap() + 2;
        let edited = format!("{}x{}", &text[..at], &text[at..]);
        let cursor = edited[..=at].encode_utf16().count();
        let (context, built) = counted(Work::ElementPath, || behavior.completion_context(&edited, cursor));
        assert!(built <= 8, "{built} paths built");
        let path: Vec<String> = context.path.iter().map(|segment| format!("{segment:?}")).collect();
        assert!(path.join("/").contains("svc_750"), "{path:?}");
    }
}
