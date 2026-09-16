//! Frontend-neutral decision requests for operations that require human input.
//!
//! The runtime emits decisions; native shells render them. This keeps conflict,
//! trust, destructive-action, and authentication policy out of GTK/AppKit code.

use std::collections::BTreeMap;
use std::fmt;

/// Stable identifier for a decision request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DecisionId(u64);

impl DecisionId {
    /// Creates a non-zero decision identifier.
    ///
    /// # Errors
    ///
    /// Returns [`DecisionError::InvalidId`] when `value` is zero.
    pub fn new(value: u64) -> Result<Self, DecisionError> {
        if value == 0 {
            Err(DecisionError::InvalidId)
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

/// Broad category of a decision request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DecisionKind {
    /// Destination already contains a regular file.
    ExistingFile,
    /// Destination already contains a directory.
    ExistingDirectory,
    /// Source and destination entry kinds conflict.
    TypeConflict,
    /// A host key is unknown or changed.
    HostTrust,
    /// A destructive delete requires confirmation.
    DeleteConfirmation,
    /// Authentication requires credentials or key selection.
    Authentication,
}

/// Action selected by the user or a remembered policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecisionChoice {
    /// Replace the destination.
    Replace,
    /// Leave the destination untouched.
    Skip,
    /// Generate a non-conflicting destination name.
    KeepBoth,
    /// Cancel the operation or request.
    Cancel,
    /// Trust only for the current connection attempt.
    TrustOnce,
    /// Persist trust when the platform/backend supports it.
    TrustAlways,
    /// Confirm a destructive delete.
    Delete,
    /// Retry the blocked operation.
    Retry,
}

/// Scope applied to a resolved decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecisionScope {
    /// Apply only to this request.
    ThisItem,
    /// Remember for matching requests for the lifetime of this center.
    AllMatching,
}

/// Pending request emitted by shared behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionRequest {
    /// Stable request identifier.
    pub id: DecisionId,
    /// Decision category.
    pub kind: DecisionKind,
    /// Short user-facing subject.
    pub subject: String,
    /// Detailed context shown by a native shell.
    pub detail: String,
    /// Choices valid for this request.
    pub allowed: Vec<DecisionChoice>,
}

/// Resolution returned to the blocked operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecisionResolution {
    /// Resolved request.
    pub id: DecisionId,
    /// Selected action.
    pub choice: DecisionChoice,
    /// Selected scope.
    pub scope: DecisionScope,
}

/// Decision-center failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecisionError {
    /// Identifier zero is reserved.
    InvalidId,
    /// Request must expose at least one action.
    NoChoices,
    /// Request does not exist.
    NotFound(DecisionId),
    /// Selected action is not valid for this request.
    ChoiceNotAllowed {
        /// Request being resolved.
        id: DecisionId,
        /// Rejected choice.
        choice: DecisionChoice,
    },
}

impl fmt::Display for DecisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId => formatter.write_str("decision id zero is reserved"),
            Self::NoChoices => {
                formatter.write_str("decision request must expose at least one choice")
            }
            Self::NotFound(id) => write!(formatter, "decision {} was not found", id.get()),
            Self::ChoiceNotAllowed { id, choice } => {
                write!(
                    formatter,
                    "choice {choice:?} is not allowed for decision {}",
                    id.get()
                )
            }
        }
    }
}

impl std::error::Error for DecisionError {}

/// In-memory request queue plus session-scoped remembered answers.
#[derive(Debug)]
pub struct DecisionCenter {
    next_id: u64,
    pending: BTreeMap<DecisionId, DecisionRequest>,
    remembered: BTreeMap<DecisionKind, DecisionChoice>,
}

impl DecisionCenter {
    /// Creates an empty decision center.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_id: 1,
            pending: BTreeMap::new(),
            remembered: BTreeMap::new(),
        }
    }

    /// Returns a remembered answer for this decision kind.
    #[must_use]
    pub fn remembered(&self, kind: DecisionKind) -> Option<DecisionChoice> {
        self.remembered.get(&kind).copied()
    }

    /// Emits a decision request unless a matching session answer exists.
    ///
    /// A remembered answer is returned immediately without adding a pending
    /// request. Otherwise the returned resolution is `None` and the caller can
    /// retrieve the pending request through [`Self::pending`].
    ///
    /// # Errors
    ///
    /// Returns [`DecisionError::NoChoices`] when no choices are supplied.
    pub fn request(
        &mut self,
        kind: DecisionKind,
        subject: impl Into<String>,
        detail: impl Into<String>,
        allowed: Vec<DecisionChoice>,
    ) -> Result<(DecisionId, Option<DecisionResolution>), DecisionError> {
        if allowed.is_empty() {
            return Err(DecisionError::NoChoices);
        }

        let id = DecisionId::new(self.next_id)?;
        self.next_id = self.next_id.saturating_add(1);

        if let Some(choice) = self.remembered(kind)
            && allowed.contains(&choice)
        {
            return Ok((
                id,
                Some(DecisionResolution {
                    id,
                    choice,
                    scope: DecisionScope::AllMatching,
                }),
            ));
        }

        self.pending.insert(
            id,
            DecisionRequest {
                id,
                kind,
                subject: subject.into(),
                detail: detail.into(),
                allowed,
            },
        );
        Ok((id, None))
    }

    /// Returns pending requests in stable identifier order.
    pub fn pending(&self) -> impl Iterator<Item = &DecisionRequest> {
        self.pending.values()
    }

    /// Resolves one pending request.
    ///
    /// # Errors
    ///
    /// Returns [`DecisionError`] when the request is absent or the selected
    /// choice is not one of its allowed choices.
    pub fn resolve(
        &mut self,
        id: DecisionId,
        choice: DecisionChoice,
        scope: DecisionScope,
    ) -> Result<DecisionResolution, DecisionError> {
        let request = self.pending.get(&id).ok_or(DecisionError::NotFound(id))?;
        if !request.allowed.contains(&choice) {
            return Err(DecisionError::ChoiceNotAllowed { id, choice });
        }
        let kind = request.kind;
        self.pending.remove(&id);
        if scope == DecisionScope::AllMatching {
            self.remembered.insert(kind, choice);
        }
        Ok(DecisionResolution { id, choice, scope })
    }

    /// Removes all remembered session policies.
    pub fn clear_remembered(&mut self) {
        self.remembered.clear();
    }
}

impl Default for DecisionCenter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{DecisionCenter, DecisionChoice, DecisionKind, DecisionScope};

    #[test]
    fn remembered_choice_resolves_matching_request_immediately()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut center = DecisionCenter::new();
        let (first, resolution) = center.request(
            DecisionKind::ExistingFile,
            "a.txt",
            "destination exists",
            vec![DecisionChoice::Replace, DecisionChoice::Skip],
        )?;
        assert!(resolution.is_none());
        center.resolve(first, DecisionChoice::Replace, DecisionScope::AllMatching)?;

        let (_, resolution) = center.request(
            DecisionKind::ExistingFile,
            "b.txt",
            "destination exists",
            vec![DecisionChoice::Replace, DecisionChoice::Skip],
        )?;
        assert_eq!(
            resolution.map(|value| value.choice),
            Some(DecisionChoice::Replace)
        );
        Ok(())
    }

    #[test]
    fn rejects_choice_not_exposed_by_request() -> Result<(), Box<dyn std::error::Error>> {
        let mut center = DecisionCenter::new();
        let (id, _) = center.request(
            DecisionKind::DeleteConfirmation,
            "folder",
            "delete folder",
            vec![DecisionChoice::Delete, DecisionChoice::Cancel],
        )?;
        assert!(
            center
                .resolve(id, DecisionChoice::Replace, DecisionScope::ThisItem)
                .is_err()
        );
        Ok(())
    }
}
