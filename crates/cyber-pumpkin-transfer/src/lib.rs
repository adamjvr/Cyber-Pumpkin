//! Transfer planning and lifecycle state for Cyber-Pumpkin.

use cyber_pumpkin_core::{BackendId, BackendPath};
use std::fmt;

/// Stable identifier for a transfer job.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TransferId(u64);

impl TransferId {
    /// Creates a transfer id. Zero is reserved as invalid.
    ///
    /// # Errors
    ///
    /// Returns [`TransferError::InvalidId`] when `value` is zero.
    pub fn new(value: u64) -> Result<Self, TransferError> {
        if value == 0 {
            return Err(TransferError::InvalidId);
        }
        Ok(Self(value))
    }

    /// Returns the numeric id.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One side of a copy operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    /// Configured backend.
    pub backend: BackendId,
    /// Backend-local path.
    pub path: BackendPath,
}

/// Immutable transfer intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferSpec {
    /// Source object.
    pub source: Endpoint,
    /// Destination object.
    pub destination: Endpoint,
}

/// Observable lifecycle state. UI and CLI render this state; they do not invent it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferState {
    /// Accepted but not yet started.
    Queued,
    /// Establishing or validating backend connections.
    Connecting,
    /// Enumerating children/metadata before transfer.
    Enumerating,
    /// Bytes are moving.
    Transferring,
    /// Content or metadata verification is running.
    Verifying,
    /// Waiting before a bounded retry.
    RetryWaiting,
    /// Explicitly paused.
    Paused,
    /// Finished successfully.
    Completed,
    /// Terminal failure.
    Failed,
    /// Terminal user cancellation.
    Cancelled,
}

impl TransferState {
    /// Returns whether no further state transition is legal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Validates a lifecycle transition.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        use TransferState::{
            Cancelled, Completed, Connecting, Enumerating, Failed, Paused, Queued, RetryWaiting,
            Transferring, Verifying,
        };
        match self {
            Queued => matches!(next, Connecting | Cancelled),
            Connecting => matches!(
                next,
                Enumerating | Transferring | RetryWaiting | Failed | Cancelled
            ),
            Enumerating => matches!(next, Transferring | RetryWaiting | Failed | Cancelled),
            Transferring => matches!(
                next,
                Verifying | RetryWaiting | Paused | Completed | Failed | Cancelled
            ),
            Verifying => matches!(next, Completed | RetryWaiting | Failed | Cancelled),
            RetryWaiting => matches!(next, Connecting | Cancelled | Failed),
            Paused => matches!(next, Connecting | Transferring | Cancelled),
            Completed | Failed | Cancelled => false,
        }
    }
}

/// Transfer state-machine errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransferError {
    /// Transfer id zero is invalid.
    InvalidId,
    /// Attempted transition violates the documented state machine.
    InvalidTransition {
        /// State the job was in when the transition was requested.
        from: TransferState,
        /// Requested destination state.
        to: TransferState,
    },
}

impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId => f.write_str("transfer id zero is reserved"),
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid transfer transition: {from:?} -> {to:?}")
            }
        }
    }
}

impl std::error::Error for TransferError {}

/// Minimal state holder. Execution will be added behind the same lifecycle contract.
#[derive(Debug)]
pub struct TransferJob {
    id: TransferId,
    spec: TransferSpec,
    state: TransferState,
}

impl TransferJob {
    /// Creates a queued job.
    #[must_use]
    pub const fn new(id: TransferId, spec: TransferSpec) -> Self {
        Self {
            id,
            spec,
            state: TransferState::Queued,
        }
    }

    /// Job identifier.
    #[must_use]
    pub const fn id(&self) -> TransferId {
        self.id
    }

    /// Immutable transfer intent.
    #[must_use]
    pub const fn spec(&self) -> &TransferSpec {
        &self.spec
    }

    /// Current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> TransferState {
        self.state
    }

    /// Applies a validated state transition.
    ///
    /// # Errors
    ///
    /// Returns [`TransferError::InvalidTransition`] when `next` is not a legal
    /// successor of the current state.
    pub fn transition(&mut self, next: TransferState) -> Result<(), TransferError> {
        if !self.state.can_transition_to(next) {
            return Err(TransferError::InvalidTransition {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Endpoint, TransferId, TransferJob, TransferSpec, TransferState};
    use cyber_pumpkin_core::{BackendId, BackendPath};

    fn job() -> Result<TransferJob, Box<dyn std::error::Error>> {
        Ok(TransferJob::new(
            TransferId::new(1)?,
            TransferSpec {
                source: Endpoint {
                    backend: BackendId::new("local")?,
                    path: BackendPath::new("/tmp/input")?,
                },
                destination: Endpoint {
                    backend: BackendId::new("production")?,
                    path: BackendPath::new("/var/www/input")?,
                },
            },
        ))
    }

    #[test]
    fn terminal_states_never_transition() {
        for state in [
            TransferState::Completed,
            TransferState::Failed,
            TransferState::Cancelled,
        ] {
            assert!(!state.can_transition_to(TransferState::Queued));
        }
    }

    #[test]
    fn happy_path_is_explicit() -> Result<(), Box<dyn std::error::Error>> {
        let mut job = job()?;
        job.transition(TransferState::Connecting)?;
        job.transition(TransferState::Transferring)?;
        job.transition(TransferState::Verifying)?;
        job.transition(TransferState::Completed)?;
        assert!(job.state().is_terminal());
        Ok(())
    }

    #[test]
    fn illegal_transition_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let mut job = job()?;
        assert!(job.transition(TransferState::Completed).is_err());
        Ok(())
    }
}
