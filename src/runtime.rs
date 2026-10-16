//! A bounded latest-value mailbox. A platform call already in flight cannot be undone.

pub struct Latest<T> {
    generation: u64,
    pending: Option<(u64, T)>,
}
impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self {
            generation: 0,
            pending: None,
        }
    }
}
impl<T> Latest<T> {
    pub fn invalidate(&mut self) -> u64 {
        self.generation += 1;
        self.pending = None;
        self.generation
    }
    pub fn replace(&mut self, value: T) -> u64 {
        let generation = self.invalidate();
        self.pending = Some((generation, value));
        generation
    }
    pub fn take(&mut self) -> Option<(u64, T)> {
        self.pending.take()
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn current(&self, generation: u64) -> bool {
        generation == self.generation
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latest_command_wins_and_old_completion_is_stale() {
        let mut queue = Latest::<u8>::default();
        let old = queue.replace(1);
        let new = queue.replace(2);
        assert_eq!(queue.take(), Some((new, 2)));
        assert!(!queue.current(old));
        assert!(queue.current(new));
        assert_eq!(queue.take(), None);
    }
    #[test]
    fn cancellation_clears_pending_and_rejects_inflight_completion() {
        let mut queue = Latest::<u8>::default();
        let running = queue.replace(1);
        queue.take();
        queue.replace(2);
        queue.invalidate();
        assert_eq!(queue.take(), None);
        assert!(!queue.current(running));
    }
}

#[derive(Default)]
pub struct Dispatch {
    last: Option<u64>,
}
impl Dispatch {
    pub fn accept(&mut self, generation: u64) -> bool {
        if self.last == Some(generation) {
            return false;
        }
        self.last = Some(generation);
        true
    }
}
#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use crate::core::{Ownership, Policy, Target};
    #[test]
    fn persistent_policy_request_dispatches_once() {
        let mut policy = Policy::new();
        let mut dispatch = Dispatch::default();
        policy.observe(Ownership::Owned, 0);
        let request = policy.tick(1000).unwrap();
        assert!(dispatch.accept(request.generation));
        assert!(!dispatch.accept(policy.tick(2000).unwrap().generation));
        policy.observe(Ownership::Absent, 3000);
        assert!(dispatch.accept(policy.tick(4000).unwrap().generation));
    }
    #[test]
    fn manual_request_is_not_resubmitted_as_automatic() {
        let mut policy = Policy::new();
        let mut dispatch = Dispatch::default();
        policy.manual(Target::Remote);
        assert!(dispatch.accept(policy.request().unwrap().generation));
        assert!(!dispatch.accept(policy.tick(1000).unwrap().generation));
        policy.sleep();
        policy.wake(2000);
        policy.observe(Ownership::Absent, 2100);
        assert!(policy.tick(4000).is_none());
    }
}

/// Stop operational state before attempting persistence; a disk failure must not undo pause.
pub fn pause<T>(policy: &mut crate::core::Policy, jobs: &mut Latest<T>, now: u64) {
    policy.set_automatic(false, now);
    jobs.invalidate();
}
#[cfg(test)]
mod pause_tests {
    use super::*;
    #[test]
    fn pause_cancels_before_persistence() {
        let mut policy = crate::core::Policy::new();
        policy.observe(crate::core::Ownership::Owned, 0);
        assert!(policy.tick(1000).is_some());
        let mut jobs = Latest::<u8>::default();
        let generation = jobs.replace(15);
        pause(&mut policy, &mut jobs, 2000);
        // These invariants already hold when a later persistence operation fails.
        assert!(!policy.automatic());
        assert!(policy.tick(3000).is_none());
        assert!(jobs.take().is_none());
        assert!(!jobs.current(generation));
    }
}
