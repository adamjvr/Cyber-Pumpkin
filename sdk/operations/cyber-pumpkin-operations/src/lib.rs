//! Generalized operation queue and dependency engine.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Stable operation identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationId(u64);

impl OperationId {
    /// Creates a non-zero operation identifier.
    ///
    /// # Errors
    ///
    /// Returns [`OperationError::InvalidId`] for zero.
    pub fn new(value: u64) -> Result<Self, OperationError> {
        if value == 0 {
            Err(OperationError::InvalidId)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the numeric identifier.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Broad operation category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationKind {
    /// Copy or transfer data.
    Copy,
    /// Delete an entry.
    Delete,
    /// Rename or move an entry.
    Rename,
    /// Create a directory.
    CreateDirectory,
    /// Execute a synchronization plan.
    Sync,
    /// Manage a remote-edit download/watch/upload lifecycle.
    RemoteEdit,
}

/// Observable operation state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationState {
    /// Accepted but waiting for dependencies or scheduler capacity.
    Queued,
    /// Actively executing.
    Running,
    /// Explicitly paused.
    Paused,
    /// Finished successfully.
    Completed,
    /// Terminal failure.
    Failed,
    /// Terminal user cancellation.
    Cancelled,
}

impl OperationState {
    /// Returns whether this is terminal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Operation progress.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OperationProgress {
    /// Completed logical units.
    pub completed_units: u64,
    /// Total logical units.
    pub total_units: Option<u64>,
    /// Completed bytes.
    pub completed_bytes: u64,
    /// Total bytes.
    pub total_bytes: Option<u64>,
}

/// One queue record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationRecord {
    /// ID.
    pub id: OperationId,
    /// Kind.
    pub kind: OperationKind,
    /// Activity label.
    pub label: String,
    /// State.
    pub state: OperationState,
    /// Dependencies.
    pub dependencies: BTreeSet<OperationId>,
    /// Progress.
    pub progress: OperationProgress,
    /// Failure message.
    pub error: Option<String>,
}

/// Queue event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationEvent {
    /// A new operation entered the queue.
    Enqueued(OperationId),
    /// An operation transitioned lifecycle state.
    StateChanged {
        /// Operation that changed.
        id: OperationId,
        /// New lifecycle state.
        state: OperationState,
    },
    /// Operation progress changed.
    Progress {
        /// Operation whose progress changed.
        id: OperationId,
        /// New progress snapshot.
        progress: OperationProgress,
    },
}

/// Queue error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationError {
    /// Identifier zero is reserved.
    InvalidId,
    /// Requested operation does not exist.
    NotFound(OperationId),
    /// Dependency refers to an unknown operation.
    UnknownDependency(OperationId),
    /// Requested lifecycle transition is illegal.
    InvalidTransition {
        /// Current lifecycle state.
        from: OperationState,
        /// Requested lifecycle state.
        to: OperationState,
    },
}

impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId => f.write_str("operation id zero is reserved"),
            Self::NotFound(id) => write!(f, "operation {} was not found", id.get()),
            Self::UnknownDependency(id) => write!(f, "dependency {} was not found", id.get()),
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid operation transition: {from:?} -> {to:?}")
            }
        }
    }
}
impl std::error::Error for OperationError {}

/// Deterministic dependency-aware queue.
#[derive(Debug, Default)]
pub struct OperationQueue {
    next_id: u64,
    records: BTreeMap<OperationId, OperationRecord>,
    events: Vec<OperationEvent>,
}

impl OperationQueue {
    /// Creates an empty queue.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_id: 1,
            records: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    /// Enqueues an operation.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown dependencies.
    pub fn enqueue(
        &mut self,
        kind: OperationKind,
        label: impl Into<String>,
        dependencies: impl IntoIterator<Item = OperationId>,
    ) -> Result<OperationId, OperationError> {
        let dependencies = dependencies.into_iter().collect::<BTreeSet<_>>();
        for dependency in &dependencies {
            if !self.records.contains_key(dependency) {
                return Err(OperationError::UnknownDependency(*dependency));
            }
        }
        let id = OperationId::new(self.next_id)?;
        self.next_id = self.next_id.saturating_add(1);
        self.records.insert(
            id,
            OperationRecord {
                id,
                kind,
                label: label.into(),
                state: OperationState::Queued,
                dependencies,
                progress: OperationProgress::default(),
                error: None,
            },
        );
        self.events.push(OperationEvent::Enqueued(id));
        Ok(id)
    }

    /// Returns one record.
    #[must_use]
    pub fn get(&self, id: OperationId) -> Option<&OperationRecord> {
        self.records.get(&id)
    }

    /// Iterates records in ID order.
    pub fn records(&self) -> impl Iterator<Item = &OperationRecord> {
        self.records.values()
    }

    /// Returns accumulated events.
    #[must_use]
    pub fn events(&self) -> &[OperationEvent] {
        &self.events
    }

    /// Returns the next runnable operation and fails dependents of failed/cancelled work.
    pub fn next_runnable(&mut self) -> Option<OperationId> {
        let ids = self.records.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let record = self.records.get(&id)?;
            if record.state != OperationState::Queued {
                continue;
            }
            let deps = record.dependencies.clone();

            let mut blocked = false;
            let mut ready = true;
            for dependency in deps {
                let state = self.records.get(&dependency)?.state;
                match state {
                    OperationState::Completed => {}
                    OperationState::Failed | OperationState::Cancelled => {
                        blocked = true;
                        ready = false;
                        break;
                    }
                    OperationState::Queued | OperationState::Running | OperationState::Paused => {
                        ready = false;
                    }
                }
            }
            if blocked {
                if let Some(record) = self.records.get_mut(&id) {
                    record.state = OperationState::Failed;
                    record.error = Some("dependency did not complete successfully".to_owned());
                }
                self.events.push(OperationEvent::StateChanged {
                    id,
                    state: OperationState::Failed,
                });
                continue;
            }
            if ready {
                return Some(id);
            }
        }
        None
    }

    /// Starts a queued operation.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid state or missing ID.
    pub fn start(&mut self, id: OperationId) -> Result<(), OperationError> {
        self.transition(id, OperationState::Running)
    }

    /// Updates operation progress.
    ///
    /// # Errors
    ///
    /// Returns an error for missing ID.
    pub fn update_progress(
        &mut self,
        id: OperationId,
        progress: OperationProgress,
    ) -> Result<(), OperationError> {
        let record = self
            .records
            .get_mut(&id)
            .ok_or(OperationError::NotFound(id))?;
        record.progress = progress;
        self.events.push(OperationEvent::Progress { id, progress });
        Ok(())
    }

    /// Pauses a running operation.
    ///
    /// # Errors
    ///
    /// Returns [`OperationError`] when the ID is missing or the transition is illegal.
    pub fn pause(&mut self, id: OperationId) -> Result<(), OperationError> {
        self.transition(id, OperationState::Paused)
    }

    /// Resumes a paused operation.
    ///
    /// # Errors
    ///
    /// Returns [`OperationError`] when the ID is missing or the transition is illegal.
    pub fn resume(&mut self, id: OperationId) -> Result<(), OperationError> {
        self.transition(id, OperationState::Running)
    }

    /// Completes a running operation.
    ///
    /// # Errors
    ///
    /// Returns [`OperationError`] when the ID is missing or the transition is illegal.
    pub fn complete(&mut self, id: OperationId) -> Result<(), OperationError> {
        self.transition(id, OperationState::Completed)
    }

    /// Fails an operation with a message.
    ///
    /// # Errors
    ///
    /// Returns [`OperationError`] when the ID is missing or the transition is illegal.
    pub fn fail(
        &mut self,
        id: OperationId,
        message: impl Into<String>,
    ) -> Result<(), OperationError> {
        self.transition(id, OperationState::Failed)?;
        if let Some(record) = self.records.get_mut(&id) {
            record.error = Some(message.into());
        }
        Ok(())
    }

    /// Cancels a non-terminal operation.
    ///
    /// # Errors
    ///
    /// Returns [`OperationError`] when the ID is missing or the transition is illegal.
    pub fn cancel(&mut self, id: OperationId) -> Result<(), OperationError> {
        self.transition(id, OperationState::Cancelled)
    }

    fn transition(&mut self, id: OperationId, next: OperationState) -> Result<(), OperationError> {
        let record = self
            .records
            .get_mut(&id)
            .ok_or(OperationError::NotFound(id))?;
        if !can_transition(record.state, next) {
            return Err(OperationError::InvalidTransition {
                from: record.state,
                to: next,
            });
        }
        record.state = next;
        self.events
            .push(OperationEvent::StateChanged { id, state: next });
        Ok(())
    }
}

const fn can_transition(from: OperationState, to: OperationState) -> bool {
    use OperationState::{Cancelled, Completed, Failed, Paused, Queued, Running};
    match from {
        Queued => matches!(to, Running | Failed | Cancelled),
        Running => matches!(to, Paused | Completed | Failed | Cancelled),
        Paused => matches!(to, Running | Failed | Cancelled),
        Completed | Failed | Cancelled => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_blocks_until_completion() -> Result<(), Box<dyn std::error::Error>> {
        let mut queue = OperationQueue::new();
        let first = queue.enqueue(OperationKind::Copy, "first", [])?;
        let second = queue.enqueue(OperationKind::Delete, "second", [first])?;
        assert_eq!(queue.next_runnable(), Some(first));
        queue.start(first)?;
        assert_eq!(queue.next_runnable(), None);
        queue.complete(first)?;
        assert_eq!(queue.next_runnable(), Some(second));
        Ok(())
    }

    #[test]
    fn failed_dependency_fails_dependent() -> Result<(), Box<dyn std::error::Error>> {
        let mut queue = OperationQueue::new();
        let first = queue.enqueue(OperationKind::Copy, "first", [])?;
        let second = queue.enqueue(OperationKind::Copy, "second", [first])?;
        queue.fail(first, "network failed")?;
        assert_eq!(queue.next_runnable(), None);
        assert_eq!(
            queue.get(second).map(|r| r.state),
            Some(OperationState::Failed)
        );
        Ok(())
    }
}
