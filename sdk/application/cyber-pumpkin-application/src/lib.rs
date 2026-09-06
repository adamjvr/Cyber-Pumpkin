//! Shared application/session state for Cyber-Pumpkin frontends.

use cyber_pumpkin_core::{BackendId, BackendPath};

/// Backend-agnostic state for one browser pane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaneSession {
    backend: BackendId,
    location: BackendPath,
    back: Vec<BackendPath>,
    forward: Vec<BackendPath>,
}

impl PaneSession {
    /// Creates a pane session at an initial backend location.
    #[must_use]
    pub const fn new(backend: BackendId, location: BackendPath) -> Self {
        Self {
            backend,
            location,
            back: Vec::new(),
            forward: Vec::new(),
        }
    }

    /// Returns the configured backend identifier.
    #[must_use]
    pub const fn backend(&self) -> &BackendId {
        &self.backend
    }

    /// Returns the current backend location.
    #[must_use]
    pub const fn location(&self) -> &BackendPath {
        &self.location
    }

    /// Returns whether Back can navigate.
    #[must_use]
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    /// Returns whether Forward can navigate.
    #[must_use]
    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Returns the location Back would select.
    #[must_use]
    pub fn back_location(&self) -> Option<&BackendPath> {
        self.back.last()
    }

    /// Returns the location Forward would select.
    #[must_use]
    pub fn forward_location(&self) -> Option<&BackendPath> {
        self.forward.last()
    }

    /// Navigates to a new location and records history.
    pub fn navigate_to(&mut self, destination: BackendPath) {
        if destination == self.location {
            return;
        }
        self.back.push(self.location.clone());
        self.location = destination;
        self.forward.clear();
    }

    /// Moves to the previous location when available.
    ///
    /// Returns `true` when the location changed.
    pub fn go_back(&mut self) -> bool {
        let Some(previous) = self.back.pop() else {
            return false;
        };
        self.forward.push(self.location.clone());
        self.location = previous;
        true
    }

    /// Moves to the next location when available.
    ///
    /// Returns `true` when the location changed.
    pub fn go_forward(&mut self) -> bool {
        let Some(next) = self.forward.pop() else {
            return false;
        };
        self.back.push(self.location.clone());
        self.location = next;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::PaneSession;
    use cyber_pumpkin_core::{BackendId, BackendPath};

    fn session() -> Result<PaneSession, Box<dyn std::error::Error>> {
        Ok(PaneSession::new(
            BackendId::new("local")?,
            BackendPath::new("/home/test")?,
        ))
    }

    #[test]
    fn navigation_records_history() -> Result<(), Box<dyn std::error::Error>> {
        let mut session = session()?;
        session.navigate_to(BackendPath::new("/home/test/Downloads")?);
        assert_eq!(session.location().as_str(), "/home/test/Downloads");
        assert!(session.can_go_back());
        assert!(!session.can_go_forward());
        Ok(())
    }

    #[test]
    fn back_and_forward_are_symmetric() -> Result<(), Box<dyn std::error::Error>> {
        let mut session = session()?;
        session.navigate_to(BackendPath::new("/home/test/Documents")?);
        assert!(session.go_back());
        assert_eq!(session.location().as_str(), "/home/test");
        assert!(session.go_forward());
        assert_eq!(session.location().as_str(), "/home/test/Documents");
        Ok(())
    }

    #[test]
    fn new_navigation_clears_forward_history() -> Result<(), Box<dyn std::error::Error>> {
        let mut session = session()?;
        session.navigate_to(BackendPath::new("/one")?);
        assert!(session.go_back());
        assert!(session.can_go_forward());
        session.navigate_to(BackendPath::new("/two")?);
        assert!(!session.can_go_forward());
        Ok(())
    }
}
