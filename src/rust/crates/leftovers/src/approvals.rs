//! One-use review approvals, bound to the app context and complete target snapshot.
use super::{AppInfo, Candidate, Kind, parse_registry_id, registry};
use cleanup_core::files::FileIdentity;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(PartialEq)]
enum Snapshot {
    Files(BTreeMap<PathBuf, FileIdentity>),
    Registry(String),
}
impl Snapshot {
    fn bytes(&self) -> usize {
        match self {
            Self::Registry(text) => text.len(),
            Self::Files(files) => files
                .keys()
                .map(|path| path.as_os_str().len() * 2 + 128)
                .sum(),
        }
    }
}
pub(super) struct Approval {
    pub original_id: String,
    context: String,
    snapshot: Snapshot,
    issued: Instant,
}
impl Approval {
    pub fn unchanged(&self, candidate: &Candidate) -> bool {
        snapshot(candidate).is_some_and(|now| now == self.snapshot)
    }
}
fn store() -> &'static Mutex<HashMap<String, Approval>> {
    static STORE: OnceLock<Mutex<HashMap<String, Approval>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}
fn snapshot(candidate: &Candidate) -> Option<Snapshot> {
    if candidate.kind == Kind::RegistryKey {
        let (hive, view, key) = parse_registry_id(&candidate.id)?;
        return registry::export(hive, view, &key).map(Snapshot::Registry);
    }
    let start = Instant::now();
    let mut pending = vec![PathBuf::from(&candidate.path)];
    let mut files = BTreeMap::new();
    while let Some(path) = pending.pop() {
        if files.len() + pending.len() >= 100_000 || start.elapsed() > Duration::from_secs(3) {
            return None;
        }
        let metadata = fs::symlink_metadata(&path).ok()?;
        let identity = FileIdentity::of(&path)?; // rejects links and junctions
        if metadata.is_dir() {
            for entry in fs::read_dir(&path).ok()? {
                if files.len() + pending.len() >= 100_000 {
                    return None;
                }
                pending.push(entry.ok()?.path());
            }
        }
        files.insert(path, identity);
    }
    Some(Snapshot::Files(files))
}
pub(super) fn issue(app: &AppInfo, candidate: &mut Candidate) -> bool {
    let Some(snapshot) = snapshot(candidate) else {
        return false;
    };
    let Ok(context) = serde_json::to_string(app) else {
        return false;
    };
    let mut approvals = store().lock().unwrap_or_else(|p| p.into_inner());
    approvals.retain(|_, approval| approval.issued.elapsed() < Duration::from_secs(900));
    let bytes = snapshot.bytes() + context.len() + 256;
    if approvals.len() >= 4096
        || bytes
            + approvals
                .values()
                .map(|a| a.snapshot.bytes() + a.context.len() + 256)
                .sum::<usize>()
            > 16 * 1024 * 1024
    {
        return false;
    }
    let token = format!("approval:{}", cleanup_operations::new_batch_id());
    approvals.insert(
        token.clone(),
        Approval {
            original_id: candidate.id.clone(),
            context,
            snapshot,
            issued: Instant::now(),
        },
    );
    candidate.id = token;
    true
}
pub(super) fn consume(app: &AppInfo, token: &str) -> Option<Approval> {
    let approval = store()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(token)?;
    (approval.issued.elapsed() < Duration::from_secs(900)
        && serde_json::to_string(app).ok()? == approval.context)
        .then_some(approval)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_descendant_and_replacement_invalidate_review() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/approval-tests")
            .join(cleanup_operations::new_batch_id());
        fs::create_dir_all(&root).unwrap();
        let file = root.join("data");
        fs::write(&file, b"old").unwrap();
        let candidate = Candidate {
            id: "folder:test".into(),
            path: root.to_string_lossy().into(),
            kind: Kind::Folder,
            confidence: super::super::Confidence::Low,
            reasons: vec![],
            size_bytes: None,
        };
        let before = snapshot(&candidate).unwrap();
        fs::write(&file, b"new contents").unwrap();
        assert!(snapshot(&candidate).is_some_and(|now| now != before));
        let before = snapshot(&candidate).unwrap();
        fs::rename(&file, root.join("previous")).unwrap();
        fs::write(&file, b"new contents").unwrap();
        assert!(snapshot(&candidate).is_some_and(|now| now != before));
        fs::remove_dir_all(root).unwrap();
    }
}
