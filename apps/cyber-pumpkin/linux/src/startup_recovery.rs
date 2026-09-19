use crate::activity;
use cyber_pumpkin_application::application_support_directory;
use cyber_pumpkin_core::BackendId;
use cyber_pumpkin_history::{HistoryKind, HistoryState};
use cyber_pumpkin_local::LocalBackend;
use cyber_pumpkin_recovery::{RecoveryJournal, discover_journals, recover_journal};
use gtk::ListBox;
use std::path::Path;

pub(crate) fn run(activity_list: &ListBox) {
    let support = application_support_directory();
    recover_local_journals(activity_list, &support.join("recovery/local"));
    report_remote_pending(
        activity_list,
        &support.join("recovery/remote"),
        &support.join("remote-edit"),
    );
}

fn recover_local_journals(activity_list: &ListBox, root: &Path) {
    let journals = match discover_journals(root) {
        Ok(journals) => journals,
        Err(error) => {
            activity::record(
                activity_list,
                None,
                HistoryKind::Recovery,
                HistoryState::Failed,
                "Startup Recovery",
                &format!("Could not scan local recovery registry: {error}"),
            );
            return;
        }
    };
    if journals.is_empty() {
        return;
    }
    let backend_id = match BackendId::new("startup-recovery-local") {
        Ok(id) => id,
        Err(error) => {
            activity::record(
                activity_list,
                None,
                HistoryKind::Recovery,
                HistoryState::Failed,
                "Startup Recovery",
                &format!("Could not initialize local recovery backend: {error}"),
            );
            return;
        }
    };
    let backend = LocalBackend::new(backend_id);
    for journal_path in journals {
        match recover_journal(&backend, &journal_path) {
            Ok(report) if report.entries_processed > 0 => activity::record(
                activity_list,
                None,
                HistoryKind::Recovery,
                HistoryState::Completed,
                "Recovered Interrupted Transfer",
                &format!(
                    "{} entries • {} restored • {} cleaned",
                    report.entries_processed, report.restored, report.cleaned
                ),
            ),
            Ok(_) => {}
            Err(error) => activity::record(
                activity_list,
                None,
                HistoryKind::Recovery,
                HistoryState::Failed,
                "Startup Recovery",
                &format!("A local recovery journal could not be replayed: {error}"),
            ),
        }
    }
}

fn report_remote_pending(activity_list: &ListBox, copy_root: &Path, remote_edit_root: &Path) {
    let mut pending_entries = 0_u64;
    let mut unreadable = 0_u64;
    for root in [copy_root, remote_edit_root] {
        let Ok(journals) = discover_journals(root) else {
            unreadable = unreadable.saturating_add(1);
            continue;
        };
        for journal_path in journals {
            match RecoveryJournal::load(&journal_path) {
                Ok(journal) => {
                    pending_entries = pending_entries
                        .saturating_add(u64::try_from(journal.entries.len()).unwrap_or(u64::MAX));
                }
                Err(_) => unreadable = unreadable.saturating_add(1),
            }
        }
    }
    if pending_entries > 0 {
        activity::record_transient(
            activity_list,
            None,
            HistoryKind::Recovery,
            HistoryState::Active,
            "Remote Recovery Pending",
            &format!(
                "{pending_entries} interrupted remote transaction(s) require a reconnect before recovery"
            ),
        );
    }
    if unreadable > 0 {
        activity::record(
            activity_list,
            None,
            HistoryKind::Recovery,
            HistoryState::Failed,
            "Recovery Registry",
            &format!("{unreadable} recovery journal location(s) could not be read"),
        );
    }
}
