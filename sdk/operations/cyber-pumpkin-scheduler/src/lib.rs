//! Concurrency admission for generalized Cyber-Pumpkin operations.
//!
//! The scheduler is intentionally separate from the operation queue. The queue
//! owns lifecycle/dependencies; this crate owns the simultaneous-work ceiling.

use cyber_pumpkin_operations::{OperationError, OperationId, OperationQueue};
use std::collections::BTreeSet;
use std::fmt;

/// Default simultaneous-operation ceiling recovered for the product UX.
pub const DEFAULT_MAX_PARALLEL: u16 = 5;
/// Minimum supported simultaneous-operation setting.
pub const MIN_MAX_PARALLEL: u16 = 1;
/// Maximum supported simultaneous-operation setting.
pub const MAX_MAX_PARALLEL: u16 = 20;

/// Scheduler configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchedulerLimits {
    max_parallel: u16,
}

impl SchedulerLimits {
    /// Creates validated scheduler limits.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError::InvalidParallelism`] outside the supported
    /// inclusive range of 1 through 20.
    pub fn new(max_parallel: u16) -> Result<Self, SchedulerError> {
        if !(MIN_MAX_PARALLEL..=MAX_MAX_PARALLEL).contains(&max_parallel) {
            return Err(SchedulerError::InvalidParallelism(max_parallel));
        }
        Ok(Self { max_parallel })
    }

    /// Returns the simultaneous-operation ceiling.
    #[must_use]
    pub const fn max_parallel(self) -> u16 {
        self.max_parallel
    }
}

impl Default for SchedulerLimits {
    fn default() -> Self {
        Self {
            max_parallel: DEFAULT_MAX_PARALLEL,
        }
    }
}

/// Scheduler failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchedulerError {
    /// Requested simultaneous-operation setting is outside 1..=20.
    InvalidParallelism(u16),
    /// Operation lifecycle update failed.
    Operation(OperationError),
    /// Caller attempted to finish an operation that was not active.
    NotActive(OperationId),
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParallelism(value) => write!(
                formatter,
                "simultaneous operation count must be 1..=20, got {value}"
            ),
            Self::Operation(error) => write!(formatter, "operation scheduler failed: {error}"),
            Self::NotActive(id) => write!(formatter, "operation {} is not active", id.get()),
        }
    }
}

impl std::error::Error for SchedulerError {}

impl From<OperationError> for SchedulerError {
    fn from(value: OperationError) -> Self {
        Self::Operation(value)
    }
}

/// Dependency-aware concurrency admission state.
#[derive(Debug)]
pub struct OperationScheduler {
    limits: SchedulerLimits,
    active: BTreeSet<OperationId>,
}

impl OperationScheduler {
    /// Creates an empty scheduler.
    #[must_use]
    pub fn new(limits: SchedulerLimits) -> Self {
        Self {
            limits,
            active: BTreeSet::new(),
        }
    }

    /// Returns the configured limits.
    #[must_use]
    pub const fn limits(&self) -> SchedulerLimits {
        self.limits
    }

    /// Returns the number of active operations.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Returns whether the operation is currently admitted.
    #[must_use]
    pub fn is_active(&self, id: OperationId) -> bool {
        self.active.contains(&id)
    }

    /// Admits as many dependency-ready queued operations as capacity allows.
    ///
    /// Returned IDs have already transitioned to `Running` in `queue`.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError`] when an operation lifecycle transition fails.
    pub fn admit_ready(
        &mut self,
        queue: &mut OperationQueue,
    ) -> Result<Vec<OperationId>, SchedulerError> {
        let capacity = usize::from(self.limits.max_parallel()).saturating_sub(self.active.len());
        let mut admitted = Vec::with_capacity(capacity);

        for _ in 0..capacity {
            let Some(id) = queue.next_runnable() else {
                break;
            };
            queue.start(id)?;
            self.active.insert(id);
            admitted.push(id);
        }

        Ok(admitted)
    }

    /// Marks an active operation complete and releases one scheduler slot.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError`] if the operation is not active or the queue
    /// transition fails.
    pub fn complete(
        &mut self,
        queue: &mut OperationQueue,
        id: OperationId,
    ) -> Result<(), SchedulerError> {
        self.require_active(id)?;
        queue.complete(id)?;
        self.active.remove(&id);
        Ok(())
    }

    /// Marks an active operation failed and releases one scheduler slot.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError`] if the operation is not active or the queue
    /// transition fails.
    pub fn fail(
        &mut self,
        queue: &mut OperationQueue,
        id: OperationId,
        message: impl Into<String>,
    ) -> Result<(), SchedulerError> {
        self.require_active(id)?;
        queue.fail(id, message)?;
        self.active.remove(&id);
        Ok(())
    }

    /// Cancels an active operation and releases one scheduler slot.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError`] if the operation is not active or the queue
    /// transition fails.
    pub fn cancel(
        &mut self,
        queue: &mut OperationQueue,
        id: OperationId,
    ) -> Result<(), SchedulerError> {
        self.require_active(id)?;
        queue.cancel(id)?;
        self.active.remove(&id);
        Ok(())
    }

    fn require_active(&self, id: OperationId) -> Result<(), SchedulerError> {
        if self.active.contains(&id) {
            Ok(())
        } else {
            Err(SchedulerError::NotActive(id))
        }
    }
}

impl Default for OperationScheduler {
    fn default() -> Self {
        Self::new(SchedulerLimits::default())
    }
}

#[cfg(test)]
mod tests {
    use super::{OperationScheduler, SchedulerLimits};
    use cyber_pumpkin_operations::{OperationKind, OperationQueue, OperationState};

    #[test]
    fn default_scheduler_admits_five_independent_operations()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut queue = OperationQueue::new();
        for index in 0..7 {
            queue.enqueue(OperationKind::Copy, format!("copy {index}"), [])?;
        }

        let mut scheduler = OperationScheduler::default();
        let admitted = scheduler.admit_ready(&mut queue)?;
        assert_eq!(admitted.len(), 5);
        assert_eq!(scheduler.active_count(), 5);
        Ok(())
    }

    #[test]
    fn dependency_is_admitted_after_parent_completion() -> Result<(), Box<dyn std::error::Error>> {
        let mut queue = OperationQueue::new();
        let parent = queue.enqueue(OperationKind::Copy, "parent", [])?;
        let child = queue.enqueue(OperationKind::Delete, "child", [parent])?;
        let mut scheduler = OperationScheduler::new(SchedulerLimits::new(1)?);

        assert_eq!(scheduler.admit_ready(&mut queue)?, vec![parent]);
        scheduler.complete(&mut queue, parent)?;
        assert_eq!(scheduler.admit_ready(&mut queue)?, vec![child]);
        assert_eq!(
            queue.get(child).map(|record| record.state),
            Some(OperationState::Running)
        );
        Ok(())
    }

    #[test]
    fn rejects_parallelism_outside_product_range() {
        assert!(SchedulerLimits::new(0).is_err());
        assert!(SchedulerLimits::new(21).is_err());
    }
}
