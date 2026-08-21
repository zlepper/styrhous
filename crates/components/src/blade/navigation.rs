#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum BladeTransition {
    Opening,
    Forward,
    Back,
    Closing,
}

#[derive(Debug, Clone)]
pub struct BladeNavigator<T> {
    current: T,
    pub(super) back_stack: Vec<T>,
    forward_stack: Vec<T>,
    pub(super) transition: Option<BladeTransition>,
    pub(super) transition_started_at: Option<f64>,
    back_steps: usize,
}

impl<T> BladeNavigator<T> {
    pub fn new(current: T) -> Self {
        Self {
            current,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            transition: Some(BladeTransition::Opening),
            transition_started_at: None,
            back_steps: 0,
        }
    }
    pub fn current(&self) -> &T {
        &self.current
    }
    pub fn current_mut(&mut self) -> &mut T {
        &mut self.current
    }
    pub fn back_stack(&self) -> &[T] {
        &self.back_stack
    }
    pub fn forward_stack(&self) -> &[T] {
        &self.forward_stack
    }
    pub fn back_stack_mut(&mut self) -> &mut Vec<T> {
        &mut self.back_stack
    }
    pub fn forward_stack_mut(&mut self) -> &mut Vec<T> {
        &mut self.forward_stack
    }
    pub fn transition(&self) -> Option<BladeTransition> {
        self.transition
    }
    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }
    pub fn can_go_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }
    pub fn push(&mut self, next: T) -> Vec<T> {
        self.back_stack
            .push(std::mem::replace(&mut self.current, next));
        self.transition = Some(BladeTransition::Forward);
        self.transition_started_at = None;
        self.back_steps = 0;
        std::mem::take(&mut self.forward_stack)
    }
    pub fn go_back(&mut self) -> bool {
        self.go_back_steps(1)
    }
    /// Move directly to an earlier entry in the back history.
    ///
    /// The resulting transition promotes the selected entry in one animation,
    /// rather than playing an animation for every intermediate entry.
    pub fn go_back_steps(&mut self, steps: usize) -> bool {
        if steps == 0 || steps > self.back_stack.len() {
            return false;
        }
        for _ in 0..steps {
            let previous = self
                .back_stack
                .pop()
                .expect("step count was checked against the back stack");
            self.forward_stack
                .push(std::mem::replace(&mut self.current, previous));
        }
        self.transition = Some(BladeTransition::Back);
        self.transition_started_at = None;
        self.back_steps = steps;
        true
    }
    pub fn go_forward(&mut self) -> bool {
        let Some(next) = self.forward_stack.pop() else {
            return false;
        };
        self.back_stack
            .push(std::mem::replace(&mut self.current, next));
        self.transition = Some(BladeTransition::Forward);
        self.transition_started_at = None;
        self.back_steps = 0;
        true
    }
    pub fn begin_close(&mut self) -> bool {
        if matches!(self.transition, Some(BladeTransition::Closing)) {
            return false;
        }
        self.transition = Some(BladeTransition::Closing);
        self.transition_started_at = None;
        self.back_steps = 0;
        true
    }
    pub fn clear_transition(&mut self) {
        self.transition = None;
        self.transition_started_at = None;
        self.back_steps = 0;
    }
    pub fn entries(&self) -> impl Iterator<Item = &T> {
        std::iter::once(&self.current)
            .chain(&self.back_stack)
            .chain(&self.forward_stack)
    }
    pub fn entries_mut(&mut self) -> impl Iterator<Item = &mut T> {
        std::iter::once(&mut self.current)
            .chain(&mut self.back_stack)
            .chain(&mut self.forward_stack)
    }

    /// Consume the navigator and return every entry it owns.  This lets a
    /// parent coordinator perform lifecycle cleanup when it replaces or
    /// closes an entire stack.
    pub fn into_entries(self) -> impl Iterator<Item = T> {
        std::iter::once(self.current)
            .chain(self.back_stack)
            .chain(self.forward_stack)
    }
    pub(super) fn seed_transition(&mut self, ctx: &egui::Context) {
        if self.transition.is_some() && self.transition_started_at.is_none() {
            self.transition_started_at = Some(ctx.input(|input| input.time));
        }
    }

    pub(super) fn back_steps(&self) -> usize {
        self.back_steps.max(1)
    }
}
