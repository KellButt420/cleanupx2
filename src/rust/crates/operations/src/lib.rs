//! Persistent operation journal.
//!
//! Every change Cleanup makes is recorded as an append-only JSON-lines event
//! log in the app's data folder. A batch is written *before* any file is
//! touched, each move is recorded as it completes, and the batch is closed at
//! the end, so an interrupted run is visible on the next launch. Undo reads the
//! same log and verifies each file before moving it back.
//!
//! JSON lines were chosen over a database: writes are single appends flushed
//! to disk, a torn final line is ignored on read, and no extra dependency is
//! needed for the volumes involved (thousands of entries).

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::windows::fs::MetadataExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use cleanup_core::time::unix_millis_now;
use cleanup_core::{CoreError, ErrorCode, Result};
use serde::{Deserialize, Serialize};

const JOURNAL_FILE: &str = "history.jsonl";
const MAX_ITEMS_IN_SUMMARY: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BatchKind {
    Organize,
    Uninstall,
    Leftovers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveOutcome {
    Moved,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UndoOutcome {
    Restored,
    Conflict,
}

/// Size and last-write time, which a copy-based cross-volume move preserves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIdentity {
    pub size_bytes: u64,
    pub modified_ticks: u64,
    #[serde(default)]
    pub volume: Option<u32>,
    #[serde(default)]
    pub file_index: Option<u64>,
}

impl FileIdentity {
    pub fn of(path: &Path) -> Option<Self> {
        let link_metadata = fs::symlink_metadata(path).ok()?;
        if link_metadata.file_attributes() & 0x400 != 0 {
            return None;
        }
        let file = File::open(path).ok()?;
        let metadata = file.metadata().ok()?;
        let mut info: windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION =
            unsafe { std::mem::zeroed() };
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandle(
                file.as_raw_handle(),
                &mut info,
            )
        } == 0
        {
            return None;
        }
        metadata.is_file().then(|| FileIdentity {
            size_bytes: metadata.len(),
            modified_ticks: metadata.last_write_time(),
            volume: Some(info.dwVolumeSerialNumber),
            file_index: Some(
                (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            ),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedMove {
    pub op_id: u32,
    pub source: String,
    pub destination: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedItem {
    pub path: String,
    pub kind: String,
    pub outcome: String,
    pub detail: Option<String>,
    pub backup_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "event",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    BatchStarted {
        batch_id: String,
        kind: BatchKind,
        title: String,
        at_unix_ms: u64,
        #[serde(default)]
        operations: Vec<PlannedMove>,
    },
    DirectoryCreated {
        batch_id: String,
        path: String,
    },
    MoveCompleted {
        batch_id: String,
        op_id: u32,
        outcome: MoveOutcome,
        destination: String,
        detail: Option<String>,
        identity: Option<FileIdentity>,
    },
    BatchFinished {
        batch_id: String,
        at_unix_ms: u64,
    },
    MoveUndone {
        batch_id: String,
        op_id: u32,
        outcome: UndoOutcome,
        detail: Option<String>,
    },
    UninstallRecorded {
        batch_id: String,
        at_unix_ms: u64,
        app_name: String,
        publisher: Option<String>,
        status: String,
        message: String,
    },
    LeftoversRemoved {
        batch_id: String,
        at_unix_ms: u64,
        app_name: String,
        items: Vec<RemovedItem>,
    },
}

impl Event {
    pub fn batch_id(&self) -> &str {
        match self {
            Event::BatchStarted { batch_id, .. }
            | Event::DirectoryCreated { batch_id, .. }
            | Event::MoveCompleted { batch_id, .. }
            | Event::BatchFinished { batch_id, .. }
            | Event::MoveUndone { batch_id, .. }
            | Event::UninstallRecorded { batch_id, .. }
            | Event::LeftoversRemoved { batch_id, .. } => batch_id,
        }
    }
}

pub struct Journal {
    path: PathBuf,
}

impl Journal {
    /// Opens (creating if needed) the journal in the app's data directory.
    pub fn open(directory: &str) -> Result<Self> {
        let directory = PathBuf::from(directory);
        if !directory.is_absolute() {
            return Err(CoreError::invalid(
                "The journal directory must be an absolute path.",
            ));
        }
        fs::create_dir_all(&directory).map_err(|e| io_error("Creating the history folder", &e))?;
        Ok(Self {
            path: directory.join(JOURNAL_FILE),
        })
    }

    pub fn directory(&self) -> &Path {
        self.path.parent().unwrap_or(&self.path)
    }

    /// Appends one event and flushes it to disk before returning.
    pub fn append(&self, event: &Event) -> Result<()> {
        static APPEND_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _lock = APPEND_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut line = serde_json::to_vec(event)
            .map_err(|e| CoreError::new(ErrorCode::Internal, e.to_string()))?;
        line.push(b'\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&self.path)
            .map_err(|e| io_error("Opening history", &e))?;
        // Separate a torn tail from the next complete event after a restart.
        if file
            .metadata()
            .map_err(|e| io_error("Reading history metadata", &e))?
            .len()
            > 0
        {
            file.seek(SeekFrom::End(-1))
                .map_err(|e| io_error("Checking history", &e))?;
            let mut last = [0u8];
            file.read_exact(&mut last)
                .map_err(|e| io_error("Checking history", &e))?;
            if last[0] != b'\n' {
                file.write_all(b"\n")
                    .map_err(|e| io_error("Recovering history tail", &e))?;
            }
        }
        file.write_all(&line)
            .map_err(|e| io_error("Writing history", &e))?;
        file.sync_data().map_err(|e| io_error("Saving history", &e))
    }

    /// Reads every intact event. A torn or unknown line (for example after a
    /// crash mid-write) is skipped rather than failing the whole history.
    pub fn read(&self) -> Result<Vec<Event>> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_error("Reading history", &e)),
        };
        Ok(BufReader::new(file)
            .lines()
            .map_while(|line| line.ok())
            .filter_map(|line| serde_json::from_str(&line).ok())
            .collect())
    }
}

pub fn new_batch_id() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    format!(
        "{:x}-{:x}-{}",
        unix_millis_now(),
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BatchStatus {
    Completed,
    CompletedWithErrors,
    /// Started but never finished; inspect before trusting the result.
    Interrupted,
    Undone,
    PartiallyUndone,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub label: String,
    pub detail: Option<String>,
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub batch_id: String,
    pub kind: BatchKind,
    pub title: String,
    pub started_at_unix_ms: u64,
    pub status: BatchStatus,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub undone: usize,
    pub can_undo: bool,
    pub message: Option<String>,
    pub items: Vec<HistoryItem>,
}

/// State of one organize batch, used by undo.
#[derive(Debug, Clone, Default)]
pub struct MoveState {
    pub op_id: u32,
    pub source: String,
    pub destination: String,
    pub moved: bool,
    pub identity: Option<FileIdentity>,
    pub undone: bool,
}

#[derive(Debug, Clone, Default)]
pub struct OrganizeBatch {
    pub moves: Vec<MoveState>,
    pub created_directories: Vec<String>,
}

pub fn organize_batch(events: &[Event], batch_id: &str) -> Option<OrganizeBatch> {
    organize_batch_events(events.iter(), batch_id)
}

fn organize_batch_events<'a>(
    events: impl Iterator<Item = &'a Event>,
    batch_id: &str,
) -> Option<OrganizeBatch> {
    let mut batch: Option<OrganizeBatch> = None;
    for event in events.filter(|e| e.batch_id() == batch_id) {
        match event {
            Event::BatchStarted {
                kind: BatchKind::Organize,
                operations,
                ..
            } => {
                batch = Some(OrganizeBatch {
                    moves: operations
                        .iter()
                        .map(|op| MoveState {
                            op_id: op.op_id,
                            source: op.source.clone(),
                            destination: op.destination.clone(),
                            ..Default::default()
                        })
                        .collect(),
                    created_directories: Vec::new(),
                });
            }
            Event::DirectoryCreated { path, .. } => {
                if let Some(batch) = batch.as_mut() {
                    batch.created_directories.push(path.clone());
                }
            }
            Event::MoveCompleted {
                op_id,
                outcome,
                destination,
                identity,
                ..
            } => {
                if let Some(state) = batch
                    .as_mut()
                    .and_then(|b| b.moves.iter_mut().find(|m| m.op_id == *op_id))
                {
                    state.moved = *outcome == MoveOutcome::Moved;
                    state.destination = destination.clone();
                    state.identity = *identity;
                }
            }
            Event::MoveUndone {
                op_id,
                outcome: UndoOutcome::Restored,
                ..
            } => {
                if let Some(state) = batch
                    .as_mut()
                    .and_then(|b| b.moves.iter_mut().find(|m| m.op_id == *op_id))
                {
                    state.undone = true;
                }
            }
            _ => {}
        }
    }
    batch
}

/// Folds the event log into one summary per batch, newest first.
pub fn summaries(events: &[Event]) -> Vec<BatchSummary> {
    // Group once using borrowed events. Scanning the complete journal for
    // every batch made refresh quadratic as informational history accumulated.
    let mut indices: HashMap<&str, usize> = HashMap::new();
    let mut groups: Vec<(&str, Vec<&Event>)> = Vec::new();
    for event in events {
        let id = event.batch_id();
        let index = *indices.entry(id).or_insert_with(|| {
            groups.push((id, Vec::new()));
            groups.len() - 1
        });
        groups[index].1.push(event);
    }
    let mut result: Vec<BatchSummary> = groups
        .iter()
        .filter_map(|(id, mine)| summarize(mine, id))
        .collect();
    result.sort_by_key(|entry| std::cmp::Reverse(entry.started_at_unix_ms));
    result
}

fn summarize(mine: &[&Event], batch_id: &str) -> Option<BatchSummary> {
    match mine.first()? {
        Event::UninstallRecorded {
            at_unix_ms,
            app_name,
            publisher,
            status,
            message,
            ..
        } => {
            let ok = status.starts_with("removed") || status == "notInstalled";
            Some(BatchSummary {
                batch_id: batch_id.to_owned(),
                kind: BatchKind::Uninstall,
                title: if ok {
                    format!("Removed {app_name}")
                } else {
                    format!("Removal of {app_name}")
                },
                started_at_unix_ms: *at_unix_ms,
                status: if ok {
                    BatchStatus::Completed
                } else {
                    BatchStatus::CompletedWithErrors
                },
                total: 1,
                succeeded: usize::from(ok),
                failed: usize::from(!ok),
                undone: 0,
                can_undo: false,
                message: Some(if message.is_empty() {
                    status.clone()
                } else {
                    message.clone()
                }),
                items: vec![HistoryItem {
                    label: app_name.clone(),
                    detail: publisher.clone(),
                    outcome: status.clone(),
                }],
            })
        }
        Event::LeftoversRemoved {
            at_unix_ms,
            app_name,
            items,
            ..
        } => {
            let failed = items
                .iter()
                .filter(|i| i.outcome == "failed" || i.outcome == "rejected")
                .count();
            Some(BatchSummary {
                batch_id: batch_id.to_owned(),
                kind: BatchKind::Leftovers,
                title: format!("Cleaned up leftovers of {app_name}"),
                started_at_unix_ms: *at_unix_ms,
                status: if failed == 0 {
                    BatchStatus::Completed
                } else {
                    BatchStatus::CompletedWithErrors
                },
                total: items.len(),
                succeeded: items.len() - failed,
                failed,
                undone: 0,
                can_undo: false,
                message: None,
                items: items
                    .iter()
                    .take(MAX_ITEMS_IN_SUMMARY)
                    .map(|i| HistoryItem {
                        label: i.path.clone(),
                        detail: i.detail.clone().or_else(|| {
                            i.backup_path
                                .clone()
                                .map(|b| format!("Registry backup: {b}"))
                        }),
                        outcome: i.outcome.clone(),
                    })
                    .collect(),
            })
        }
        Event::BatchStarted {
            kind,
            title,
            at_unix_ms,
            ..
        } => {
            let finished = mine
                .iter()
                .any(|e| matches!(e, Event::BatchFinished { .. }));
            let state = organize_batch_events(mine.iter().copied(), batch_id).unwrap_or_default();
            let failures: Vec<(&u32, &Option<String>)> = mine
                .iter()
                .filter_map(|e| match e {
                    Event::MoveCompleted {
                        op_id,
                        outcome: MoveOutcome::Failed,
                        detail,
                        ..
                    } => Some((op_id, detail)),
                    _ => None,
                })
                .collect();
            let moved = state.moves.iter().filter(|m| m.moved).count();
            let undone = state.moves.iter().filter(|m| m.undone).count();
            let status = if undone > 0 && undone == moved {
                BatchStatus::Undone
            } else if undone > 0 {
                BatchStatus::PartiallyUndone
            } else if !finished {
                BatchStatus::Interrupted
            } else if failures.is_empty() {
                BatchStatus::Completed
            } else {
                BatchStatus::CompletedWithErrors
            };
            let items = state
                .moves
                .iter()
                .take(MAX_ITEMS_IN_SUMMARY)
                .map(|m| {
                    let failure = failures.iter().find(|(id, _)| **id == m.op_id);
                    HistoryItem {
                        label: file_name(&m.source),
                        detail: Some(if m.moved || failure.is_none() {
                            format!("{} → {}", m.source, m.destination)
                        } else {
                            failure.and_then(|(_, d)| (*d).clone()).unwrap_or_default()
                        }),
                        outcome: if m.undone {
                            "restored".into()
                        } else if m.moved {
                            "moved".into()
                        } else if failure.is_some() {
                            "failed".into()
                        } else {
                            if finished {
                                "notStarted"
                            } else {
                                "pending — inspect both paths"
                            }
                            .into()
                        },
                    }
                })
                .collect();
            Some(BatchSummary {
                batch_id: batch_id.to_owned(),
                kind: *kind,
                title: title.clone(),
                started_at_unix_ms: *at_unix_ms,
                status,
                total: state.moves.len(),
                succeeded: moved,
                failed: failures.len(),
                undone,
                can_undo: *kind == BatchKind::Organize
                    && state.moves.iter().any(|m| m.moved && !m.undone),
                message: None,
                items,
            })
        }
        _ => None,
    }
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned())
}

fn io_error(context: &str, error: &std::io::Error) -> CoreError {
    match error.raw_os_error() {
        Some(code) => CoreError::win32(context, code as u32),
        None => CoreError::new(ErrorCode::Internal, format!("{context}: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_journal(name: &str) -> (Journal, PathBuf) {
        let dir = std::env::temp_dir().join(format!("cleanup-ops-{name}-{}", new_batch_id()));
        (Journal::open(&dir.to_string_lossy()).unwrap(), dir)
    }

    fn started(id: &str) -> Event {
        Event::BatchStarted {
            batch_id: id.into(),
            kind: BatchKind::Organize,
            title: "Organized Downloads".into(),
            at_unix_ms: 1,
            operations: vec![
                PlannedMove {
                    op_id: 1,
                    source: r"C:\d\a.png".into(),
                    destination: r"C:\d\Images\a.png".into(),
                },
                PlannedMove {
                    op_id: 2,
                    source: r"C:\d\b.pdf".into(),
                    destination: r"C:\d\Documents\b.pdf".into(),
                },
            ],
        }
    }

    fn moved(id: &str, op: u32) -> Event {
        Event::MoveCompleted {
            batch_id: id.into(),
            op_id: op,
            outcome: MoveOutcome::Moved,
            destination: "x".into(),
            detail: None,
            identity: Some(FileIdentity {
                size_bytes: 1,
                modified_ticks: 2,
                volume: None,
                file_index: None,
            }),
        }
    }

    #[test]
    fn events_round_trip_and_torn_lines_are_ignored() {
        let (journal, dir) = temp_journal("roundtrip");
        journal.append(&started("b1")).unwrap();
        journal.append(&moved("b1", 1)).unwrap();
        // Simulate a crash in the middle of writing the next line.
        OpenOptions::new()
            .append(true)
            .open(dir.join(JOURNAL_FILE))
            .unwrap()
            .write_all(b"{\"event\":\"moveComp")
            .unwrap();
        let events = journal.read().unwrap();
        assert_eq!(events.len(), 2);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unfinished_batches_are_reported_as_interrupted() {
        let events = vec![started("b1"), moved("b1", 1)];
        let summary = &summaries(&events)[0];
        assert_eq!(summary.status, BatchStatus::Interrupted);
        assert_eq!((summary.total, summary.succeeded), (2, 1));
        assert!(summary.can_undo);
    }

    #[test]
    fn undo_state_tracks_restored_moves() {
        let events = vec![
            started("b1"),
            moved("b1", 1),
            moved("b1", 2),
            Event::BatchFinished {
                batch_id: "b1".into(),
                at_unix_ms: 2,
            },
            Event::MoveUndone {
                batch_id: "b1".into(),
                op_id: 1,
                outcome: UndoOutcome::Restored,
                detail: None,
            },
        ];
        assert_eq!(summaries(&events)[0].status, BatchStatus::PartiallyUndone);
        let batch = organize_batch(&events, "b1").unwrap();
        assert!(batch.moves[0].undone && !batch.moves[1].undone);
        let mut all = events.clone();
        all.push(Event::MoveUndone {
            batch_id: "b1".into(),
            op_id: 2,
            outcome: UndoOutcome::Restored,
            detail: None,
        });
        let summary = &summaries(&all)[0];
        assert_eq!(summary.status, BatchStatus::Undone);
        assert!(!summary.can_undo);
    }

    #[test]
    fn uninstall_records_are_informational() {
        let events = vec![Event::UninstallRecorded {
            batch_id: "u1".into(),
            at_unix_ms: 5,
            app_name: "App".into(),
            publisher: None,
            status: "removed".into(),
            message: "Removed.".into(),
        }];
        let summary = &summaries(&events)[0];
        assert_eq!(summary.kind, BatchKind::Uninstall);
        assert!(!summary.can_undo);
    }

    #[test]
    fn grouping_preserves_interleaved_batches_and_stable_timestamp_order() {
        let events = vec![
            started("first"),
            started("second"),
            moved("second", 1),
            moved("first", 2),
            Event::BatchFinished {
                batch_id: "second".into(),
                at_unix_ms: 2,
            },
            Event::MoveUndone {
                batch_id: "second".into(),
                op_id: 1,
                outcome: UndoOutcome::Restored,
                detail: None,
            },
            Event::UninstallRecorded {
                batch_id: "newest".into(),
                at_unix_ms: 10,
                app_name: "Fixture".into(),
                publisher: None,
                status: "removed".into(),
                message: "Informational".into(),
            },
            // An orphan event must not create a successful batch.
            moved("orphan", 1),
        ];
        let grouped = summaries(&events);
        assert_eq!(
            grouped
                .iter()
                .map(|s| s.batch_id.as_str())
                .collect::<Vec<_>>(),
            ["newest", "first", "second"]
        );
        assert_eq!(grouped[1].status, BatchStatus::Interrupted);
        assert_eq!(grouped[1].succeeded, 1);
        assert_eq!(grouped[2].status, BatchStatus::Undone);
        assert!(!grouped[2].can_undo);

        let mut reference = ["first", "second", "newest", "orphan"]
            .into_iter()
            .filter_map(|id| {
                let mine: Vec<&Event> = events
                    .iter()
                    .filter(|event| event.batch_id() == id)
                    .collect();
                summarize(&mine, id)
            })
            .collect::<Vec<_>>();
        reference.sort_by_key(|s| std::cmp::Reverse(s.started_at_unix_ms));
        assert_eq!(
            serde_json::to_value(grouped).unwrap(),
            serde_json::to_value(reference).unwrap()
        );
    }
}
