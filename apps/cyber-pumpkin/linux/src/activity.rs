use adw::prelude::*;
use cyber_pumpkin_application::{AppPreferences, application_support_directory};
use cyber_pumpkin_history::{HistoryEntry, HistoryKind, HistoryLog, HistoryState};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ACTIVITY_ENTRIES: usize = 200;

pub(crate) fn create_activity_list() -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);

    match load_history() {
        Ok(history) if !history.entries.is_empty() => {
            for entry in history.entries.iter().rev() {
                append_history_row(&list, entry, false);
            }
        }
        Ok(_) => append_empty_row(&list),
        Err(error) => {
            append_empty_row(&list);
            eprintln!("failed to load activity history: {error}");
        }
    }

    list
}

pub(crate) fn record(
    list: &gtk::ListBox,
    operation_id: Option<u64>,
    kind: HistoryKind,
    state: HistoryState,
    label: &str,
    detail: &str,
) {
    remove_empty_row(list);

    let entry = HistoryEntry::new(operation_id, kind, state, label, detail, now_unix_seconds());
    append_history_row(list, &entry, true);

    let keep_activity = AppPreferences::load_default()
        .map_or(true, |preferences| preferences.transfers.keep_activity);
    if !keep_activity {
        return;
    }

    match load_history() {
        Ok(mut history) => {
            history.append(entry, MAX_ACTIVITY_ENTRIES);
            if let Err(error) = history.save(&history_path()) {
                eprintln!("failed to save activity history: {error}");
            }
        }
        Err(error) => eprintln!("failed to update activity history: {error}"),
    }
}

/// Adds a UI-only activity event without persisting it to history.
///
/// This is used for live states such as an active Remote Edit watcher. A crash
/// or restart therefore cannot leave a historical row claiming a dead session
/// is still active.
pub(crate) fn record_transient(
    list: &gtk::ListBox,
    operation_id: Option<u64>,
    kind: HistoryKind,
    state: HistoryState,
    label: &str,
    detail: &str,
) {
    remove_empty_row(list);
    let entry = HistoryEntry::new(operation_id, kind, state, label, detail, now_unix_seconds());
    append_history_row(list, &entry, true);
}

pub(crate) fn clear(list: &gtk::ListBox) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    append_empty_row(list);

    let mut history = HistoryLog::default();
    history.clear();
    if let Err(error) = history.save(&history_path()) {
        eprintln!("failed to clear activity history: {error}");
    }
}

fn load_history() -> Result<HistoryLog, std::io::Error> {
    HistoryLog::load(&history_path())
}

fn history_path() -> std::path::PathBuf {
    application_support_directory().join("history.json")
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn append_history_row(list: &gtk::ListBox, entry: &HistoryEntry, prepend: bool) {
    let id = entry
        .operation_id
        .map_or_else(|| "—".to_owned(), |value| format!("#{value}"));
    let label = gtk::Label::new(Some(&format!(
        "{id} • {} • {} • {}",
        entry.label,
        state_name(entry.state),
        entry.detail,
    )));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_margin_top(4);
    label.set_margin_bottom(4);
    label.set_margin_start(8);
    label.set_margin_end(8);

    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_child(Some(&label));
    if prepend {
        list.prepend(&row);
    } else {
        list.append(&row);
    }
}

const fn state_name(state: HistoryState) -> &'static str {
    match state {
        HistoryState::Active => "Active",
        HistoryState::Completed => "Completed",
        HistoryState::Cancelled => "Cancelled",
        HistoryState::Failed => "Failed",
    }
}

fn append_empty_row(list: &gtk::ListBox) {
    let label = gtk::Label::new(Some("No transfer activity yet."));
    label.set_xalign(0.0);
    label.add_css_class("dim-label");
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_child(Some(&label));
    list.append(&row);
}

fn remove_empty_row(list: &gtk::ListBox) {
    let Some(row) = list.row_at_index(0) else {
        return;
    };
    let Some(child) = row.child() else {
        return;
    };
    let Ok(label) = child.downcast::<gtk::Label>() else {
        return;
    };
    if label.text() == "No transfer activity yet." {
        list.remove(&row);
    }
}
