//! Bounded history pages. New events live in individual batch logs; legacy
//! history is streamed without rewriting or discarding existing audit data.
use super::*;
use std::collections::BTreeSet;
const MAX_LINE: u64 = 16 * 1024 * 1024;
const MAX_BATCH: usize = 64 * 1024 * 1024;
pub const PAGE_SIZE: usize = 20;

fn visit(path: &Path, mut emit: impl FnMut(Event, usize) -> Result<bool>) -> Result<()> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io_error("Reading history", &e)),
    };
    let mut reader = BufReader::new(file);
    loop {
        let mut bytes = Vec::new();
        Read::by_ref(&mut reader)
            .take(MAX_LINE + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|e| io_error("Reading history", &e))?;
        if bytes.is_empty() {
            return Ok(());
        }
        if bytes.len() as u64 > MAX_LINE {
            return Err(CoreError::invalid(
                "A history event exceeds the safety limit. Preserve the log for manual review.",
            ));
        }
        if let Ok(event) = serde_json::from_slice(&bytes) {
            if !emit(event, bytes.len())? {
                return Ok(());
            }
        }
    }
}
fn timestamp(event: &Event) -> Option<u64> {
    match event {
        Event::BatchStarted { at_unix_ms, .. }
        | Event::UninstallRecorded { at_unix_ms, .. }
        | Event::LeftoversRemoved { at_unix_ms, .. } => Some(*at_unix_ms),
        _ => None,
    }
}
impl Journal {
    pub(super) fn batch_path(&self, id: &str) -> Result<PathBuf> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(CoreError::invalid("Invalid history batch identifier."));
        }
        let folder = self.directory().join("batches");
        fs::create_dir_all(&folder).map_err(|e| io_error("Creating batch history", &e))?;
        Ok(folder.join(format!("{id}.jsonl")))
    }
    pub(super) fn read_selected(&self, id: Option<&str>) -> Result<Vec<Event>> {
        let mut events = Vec::new();
        let mut bytes = 0;
        let mut collect = |event: Event, size: usize| {
            if id.is_none_or(|id| event.batch_id() == id) {
                bytes += size;
                if bytes > MAX_BATCH {
                    return Err(CoreError::invalid(
                        "This history batch is too large to load safely. Preserve its log for manual review.",
                    ));
                }
                events.push(event);
            }
            Ok(true)
        };
        visit(&self.path, &mut collect)?;
        if let Some(id) = id {
            visit(&self.batch_path(id)?, &mut collect)?;
        } else if let Ok(files) = fs::read_dir(self.directory().join("batches")) {
            for file in files {
                let path = file.map_err(|e| io_error("Listing history", &e))?.path();
                if path.extension().is_some_and(|e| e == "jsonl") {
                    visit(&path, &mut collect)?;
                }
            }
        }
        Ok(events)
    }
    pub fn read_batch(&self, id: &str) -> Result<Vec<Event>> {
        self.batch_path(id)?;
        self.read_selected(Some(id))
    }
    pub fn recent(&self, before: Option<&str>) -> Result<Vec<BatchSummary>> {
        let cursor = if let Some(id) = before {
            let events = self.read_batch(id)?;
            Some((
                events.iter().find_map(timestamp).ok_or_else(|| {
                    CoreError::invalid("History page no longer exists. Refresh history.")
                })?,
                id.to_owned(),
            ))
        } else {
            None
        };
        let mut newest = BTreeSet::new();
        let mut remember = |event: Event, _: usize| {
            if let Some(at) = timestamp(&event) {
                let key = (at, event.batch_id().to_owned());
                if cursor.as_ref().is_none_or(|cursor| &key < cursor) {
                    newest.insert(key);
                    if newest.len() > PAGE_SIZE {
                        newest.pop_first();
                    }
                }
            }
            Ok(true)
        };
        visit(&self.path, &mut remember)?;
        if let Ok(files) = fs::read_dir(self.directory().join("batches")) {
            for file in files {
                let path = file.map_err(|e| io_error("Listing history", &e))?.path();
                if path.extension().is_some_and(|e| e == "jsonl") {
                    visit(&path, |event, size| {
                        remember(event, size)?;
                        Ok(false)
                    })?;
                }
            }
        }
        // Stream legacy events once for this page rather than rescanning the
        // lifetime log separately for every selected batch.
        let selected: std::collections::HashSet<&str> =
            newest.iter().map(|(_, id)| id.as_str()).collect();
        let mut legacy: HashMap<String, Vec<Event>> = HashMap::new();
        let mut legacy_bytes = 0;
        visit(&self.path, |event, size| {
            if selected.contains(event.batch_id()) {
                legacy_bytes += size;
                if legacy_bytes > MAX_BATCH {
                    return Err(CoreError::invalid(
                        "The legacy history page is too large to load safely. Preserve its log for manual review.",
                    ));
                }
                legacy
                    .entry(event.batch_id().to_owned())
                    .or_default()
                    .push(event);
            }
            Ok(true)
        })?;
        let mut page = Vec::new();
        let mut page_bytes = 0;
        for (_, id) in newest.into_iter().rev() {
            let mut events = legacy.remove(&id).unwrap_or_default();
            let mut batch_bytes = 0;
            visit(&self.batch_path(&id)?, |event, size| {
                batch_bytes += size;
                if batch_bytes > MAX_BATCH {
                    return Err(CoreError::invalid(
                        "The history batch is too large to load safely.",
                    ));
                }
                events.push(event);
                Ok(true)
            })?;
            let entries = summaries(&events);
            page_bytes += serde_json::to_vec(&entries)
                .map_err(|e| CoreError::invalid(e.to_string()))?
                .len();
            if page_bytes > 16 * 1024 * 1024 {
                return Err(CoreError::invalid(
                    "This history page is too large to display safely. Preserve the batch logs for manual review.",
                ));
            }
            page.extend(entries);
        }
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_keep_legacy_and_new_batches_without_loading_lifetime_events() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/history-tests")
            .join(new_batch_id());
        let journal = Journal::open(root.to_str().unwrap()).unwrap();
        for i in 0..45 {
            journal
                .append(&Event::UninstallRecorded {
                    batch_id: format!("b{i}"),
                    at_unix_ms: i,
                    app_name: "fixture".into(),
                    publisher: None,
                    status: "removed".into(),
                    message: "test".into(),
                })
                .unwrap();
        }
        let first = journal.recent(None).unwrap();
        assert_eq!(first.len(), 20);
        assert_eq!(first[0].batch_id, "b44");
        let second = journal.recent(Some(&first[19].batch_id)).unwrap();
        assert_eq!(second.len(), 20);
        assert_eq!(second[0].batch_id, "b24");
        assert_eq!(journal.read_batch("b1").unwrap().len(), 1);
        let legacy = Event::UninstallRecorded {
            batch_id: "legacy".into(),
            at_unix_ms: 1000,
            app_name: "legacy fixture".into(),
            publisher: None,
            status: "removed".into(),
            message: "preserved".into(),
        };
        fs::write(&journal.path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(journal.recent(None).unwrap()[0].batch_id, "legacy");
        assert_eq!(journal.read_batch("legacy").unwrap().len(), 1);
        assert!(journal.read_batch("../escape").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
