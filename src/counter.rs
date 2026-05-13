use crate::types::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterEvent {
    None,
    Incremented,
    Armed,
}

/// State machine that prevents the counter from running away while the
/// shiny image stays on screen.
///
/// Rules (from spec):
/// * When **every** picker matches its target AND the state is armed,
///   the counter is incremented and the machine disarms.
/// * It rearms only when **every** picker sample differs from its target,
///   i.e. the user has fully left the shiny screen.
#[derive(Debug, Clone)]
pub struct CounterState {
    armed: bool,
}

impl Default for CounterState {
    fn default() -> Self {
        Self { armed: true }
    }
}

impl CounterState {
    pub fn is_armed(&self) -> bool {
        self.armed
    }

    pub fn reset(&mut self) {
        self.armed = true;
    }

    pub fn tick(
        &mut self,
        sample: &[Color],
        target: &[Color],
        tolerance: u8,
        count: &mut u32,
    ) -> CounterEvent {
        debug_assert_eq!(sample.len(), target.len());
        if sample.is_empty() || sample.len() != target.len() {
            return CounterEvent::None;
        }

        let all_match = sample
            .iter()
            .zip(target.iter())
            .all(|(s, t)| s.matches(*t, tolerance));

        if all_match {
            if self.armed {
                *count = count.saturating_add(1);
                self.armed = false;
                return CounterEvent::Incremented;
            }
            return CounterEvent::None;
        }

        let all_differ = sample
            .iter()
            .zip(target.iter())
            .all(|(s, t)| !s.matches(*t, tolerance));

        if all_differ && !self.armed {
            self.armed = true;
            return CounterEvent::Armed;
        }
        CounterEvent::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(r: u8, g: u8, b: u8) -> Color {
        Color::new(r, g, b)
    }

    #[test]
    fn first_match_increments_and_disarms() {
        let mut state = CounterState::default();
        let mut count = 0;
        let target = vec![c(255, 0, 0), c(0, 255, 0), c(0, 0, 255)];
        assert_eq!(
            state.tick(&target, &target, 0, &mut count),
            CounterEvent::Incremented
        );
        assert_eq!(count, 1);
        assert!(!state.is_armed());
    }

    #[test]
    fn consecutive_identical_match_does_not_double_count() {
        let mut state = CounterState::default();
        let mut count = 0;
        let target = vec![c(10, 20, 30), c(40, 50, 60), c(70, 80, 90)];
        for _ in 0..5 {
            state.tick(&target, &target, 0, &mut count);
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn requires_all_to_differ_before_rearm() {
        let mut state = CounterState::default();
        let mut count = 0;
        let target = vec![c(10, 20, 30), c(40, 50, 60), c(70, 80, 90)];
        state.tick(&target, &target, 0, &mut count);

        let partial = vec![c(0, 0, 0), c(0, 0, 0), c(70, 80, 90)];
        state.tick(&partial, &target, 0, &mut count);
        assert!(!state.is_armed());

        let all_diff = vec![c(0, 0, 0); 3];
        assert_eq!(
            state.tick(&all_diff, &target, 0, &mut count),
            CounterEvent::Armed
        );

        let evt = state.tick(&target, &target, 0, &mut count);
        assert_eq!(evt, CounterEvent::Incremented);
        assert_eq!(count, 2);
    }

    #[test]
    fn works_with_more_than_three_pickers() {
        let mut state = CounterState::default();
        let mut count = 0;
        let target: Vec<Color> = (0..6).map(|i| c(i * 30, i * 30, i * 30)).collect();
        state.tick(&target, &target, 0, &mut count);
        assert_eq!(count, 1);
        let off: Vec<Color> = (0..6).map(|_| c(200, 200, 200)).collect();
        state.tick(&off, &target, 0, &mut count);
        state.tick(&target, &target, 0, &mut count);
        assert_eq!(count, 2);
    }

    #[test]
    fn tolerance_accepts_near_matches() {
        let mut state = CounterState::default();
        let mut count = 0;
        let target = vec![c(100, 100, 100); 3];
        let near = vec![c(110, 90, 105), c(95, 100, 100), c(100, 100, 100)];
        state.tick(&near, &target, 15, &mut count);
        assert_eq!(count, 1);
    }

    #[test]
    fn reset_rearms() {
        let mut state = CounterState::default();
        let mut count = 0;
        let t = vec![c(1, 2, 3); 3];
        state.tick(&t, &t, 0, &mut count);
        assert!(!state.is_armed());
        state.reset();
        assert!(state.is_armed());
    }
}
