//! A walk over a text's units (its tokens, its lines) from the top, its
//! state kept every [`EVERY`] units: the state at any unit is the last one
//! kept before it walked on to there. A language reading the path at many
//! places of one text (host problems by range placed through an Edit-as
//! view) walks it once this way, where each place alone walked it from the
//! top.
#![cfg_attr(
    not(any(feature = "yaml", feature = "json", feature = "toml", feature = "xml")),
    allow(
        dead_code,
        reason = "the YAML, JSON, TOML and XML readers walk this way"
    )
)]

/// Units walked between two kept states.
pub(crate) const EVERY: usize = 64;

/// The states of one walk, at every [`EVERY`]th unit.
pub(crate) struct Checkpoints<S> {
    states: Vec<S>,
}

impl<S: Clone> Checkpoints<S> {
    /// Walks `units` units from `start`, `step` taking the state past one.
    pub(crate) fn walk(start: S, units: usize, mut step: impl FnMut(&mut S, usize)) -> Self {
        let mut states = Vec::with_capacity(units / EVERY + 1);
        let mut state = start;
        for unit in 0..units {
            if unit.is_multiple_of(EVERY) {
                states.push(state.clone());
            }
            step(&mut state, unit);
        }
        if units.is_multiple_of(EVERY) {
            states.push(state);
        }
        Self { states }
    }

    /// The state before unit `unit` (at most the number walked): the one
    /// kept last before it, walked on with `step`.
    pub(crate) fn before(&self, unit: usize, mut step: impl FnMut(&mut S, usize)) -> S {
        let kept = (unit / EVERY).min(self.states.len() - 1);
        let mut state = self.states[kept].clone();
        for at in kept * EVERY..unit {
            step(&mut state, at);
        }
        state
    }
}

#[cfg(test)]
mod tests {
    use super::{Checkpoints, EVERY};

    /// Every state read from the kept ones is the one walked from the top.
    #[test]
    fn states_read_from_the_kept_ones_are_the_ones_walked_from_the_top() {
        let units: Vec<u64> = (0..(3 * EVERY as u64 + 5))
            .map(|unit| unit * 7 % 11)
            .collect();
        let step = |state: &mut (u64, Vec<u64>), at: usize| {
            state.0 = state.0.wrapping_mul(31).wrapping_add(units[at]);
            state.1.push(units[at]);
        };
        for count in [0, 1, EVERY, EVERY + 1, units.len()] {
            let kept = Checkpoints::walk((0, Vec::new()), count, step);
            for unit in 0..=count {
                let mut alone = (0, Vec::new());
                for at in 0..unit {
                    step(&mut alone, at);
                }
                assert_eq!(kept.before(unit, step), alone, "{count} units, at {unit}");
            }
        }
    }
}
