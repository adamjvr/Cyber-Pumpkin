//! Reusable file-rule evaluation shared by transfer and synchronization.

use cyber_pumpkin_core::EntryKind;

/// Rule field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleField {
    /// File or directory name.
    Name,
    /// Final filename extension without the leading dot.
    Extension,
    /// Backend-relative path.
    Path,
    /// Stable entry-kind category.
    Kind,
}

/// Rule comparison.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleOperator {
    /// Exact case-sensitive equality.
    Is,
    /// Exact case-sensitive inequality.
    IsNot,
    /// Substring containment.
    Contains,
    /// Prefix match.
    StartsWith,
    /// Suffix match.
    EndsWith,
}

/// One predicate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleCondition {
    /// Field.
    pub field: RuleField,
    /// Operator.
    pub operator: RuleOperator,
    /// Comparison value.
    pub value: String,
}

/// Condition composition mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleMatchMode {
    /// At least one condition must match.
    Any,
    /// Every condition must match.
    All,
}

/// Rule decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleDecision {
    /// Keep the entry eligible.
    Include,
    /// Skip the entry.
    Skip,
}

/// Rule evaluation target.
#[derive(Clone, Copy, Debug)]
pub struct RuleTarget<'a> {
    /// Name.
    pub name: &'a str,
    /// Relative path.
    pub path: &'a str,
    /// Kind.
    pub kind: EntryKind,
}

/// Named reusable rule set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleSet {
    /// Name.
    pub name: String,
    /// Enabled state.
    pub enabled: bool,
    /// Any/all composition.
    pub mode: RuleMatchMode,
    /// Conditions.
    pub conditions: Vec<RuleCondition>,
    /// Decision when matched.
    pub decision: RuleDecision,
}

impl RuleSet {
    /// Tests a target against this set.
    #[must_use]
    pub fn matches(&self, target: RuleTarget<'_>) -> bool {
        if !self.enabled || self.conditions.is_empty() {
            return false;
        }
        match self.mode {
            RuleMatchMode::Any => self.conditions.iter().any(|c| condition_matches(c, target)),
            RuleMatchMode::All => self.conditions.iter().all(|c| condition_matches(c, target)),
        }
    }
}

/// Evaluates ordered rule sets. First match wins, otherwise include.
#[must_use]
pub fn evaluate(rule_sets: &[RuleSet], target: RuleTarget<'_>) -> RuleDecision {
    rule_sets
        .iter()
        .find(|set| set.matches(target))
        .map_or(RuleDecision::Include, |set| set.decision)
}

fn condition_matches(condition: &RuleCondition, target: RuleTarget<'_>) -> bool {
    let kind;
    let extension;
    let actual = match condition.field {
        RuleField::Name => target.name,
        RuleField::Path => target.path,
        RuleField::Kind => {
            kind = kind_name(target.kind);
            kind
        }
        RuleField::Extension => {
            extension = target
                .name
                .rsplit_once('.')
                .map_or("", |(_, suffix)| suffix);
            extension
        }
    };
    match condition.operator {
        RuleOperator::Is => actual == condition.value,
        RuleOperator::IsNot => actual != condition.value,
        RuleOperator::Contains => actual.contains(&condition.value),
        RuleOperator::StartsWith => actual.starts_with(&condition.value),
        RuleOperator::EndsWith => actual.ends_with(&condition.value),
    }
}

const fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "file",
        EntryKind::Directory => "directory",
        EntryKind::Symlink => "symlink",
        EntryKind::Other => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cyber_pumpkin_core::EntryKind;

    #[test]
    fn git_directory_can_be_skipped() {
        let rules = vec![RuleSet {
            name: "Source Control".to_owned(),
            enabled: true,
            mode: RuleMatchMode::Any,
            conditions: vec![RuleCondition {
                field: RuleField::Name,
                operator: RuleOperator::Is,
                value: ".git".to_owned(),
            }],
            decision: RuleDecision::Skip,
        }];
        assert_eq!(
            evaluate(
                &rules,
                RuleTarget {
                    name: ".git",
                    path: "project/.git",
                    kind: EntryKind::Directory
                }
            ),
            RuleDecision::Skip
        );
    }
}
