//! Leftover review after an official uninstall.
//!
//! Candidates are folders, shortcuts, and registry keys whose *names* match
//! the removed app, found only in well-known per-app locations. Each comes
//! with the reasons it was matched and a confidence level based on how many
//! independent signals agree; a name match alone is never "high".
//!
//! Safety rules: protected and shared roots are never candidates; links and
//! junctions are never followed; names shared with another installed app are
//! not used for matching; another app's install folder is never a candidate.
//! Removal re-runs the scan and acts only on items that still match. Files and
//! folders go to the Recycle Bin (Windows asks before deleting permanently);
//! registry keys are exported to a `.reg` backup before deletion.

mod registry;

use std::collections::HashSet;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::time::Instant;

use cleanup_core::Result;
use cleanup_core::model::{Hive, RegView};
use cleanup_core::paths::{
    DirectoryGuard, KnownFolder, is_drive_root, is_plain_absolute, is_reparse_point, is_within,
    known_folder, protected_roots, same_path,
};
use cleanup_core::time::unix_millis_now;
use cleanup_operations::{Event, Journal, RemovedItem, new_batch_id};
use serde::{Deserialize, Serialize};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_WANTNUKEWARNING,
    SHFILEOPSTRUCTW, SHFileOperationW,
};

const GENERIC_NAMES: &[&str] = &[
    "app",
    "apps",
    "cache",
    "common",
    "data",
    "default",
    "launcher",
    "logs",
    "microsoft",
    "packages",
    "programs",
    "setup",
    "installer",
    "shared",
    "system",
    "temp",
    "tools",
    "update",
    "updater",
    "user",
    "users",
    "windows",
    "windowsapps",
    "software",
    "client",
    "service",
];
const STRUCTURAL_KEYS: &[&str] = &[
    "classes",
    "clients",
    "microsoft",
    "policies",
    "wow6432node",
    "registeredapplications",
    "odbc",
    "windows",
];
const LEGAL_SUFFIXES: &[&str] = &[
    "inc",
    "llc",
    "ltd",
    "limited",
    "corp",
    "corporation",
    "co",
    "company",
    "gmbh",
    "ag",
    "sa",
    "sarl",
    "srl",
    "bv",
    "pty",
    "plc",
    "kk",
    "oy",
    "ab",
    "as",
];
const ARCHITECTURE_WORDS: &[&str] = &[
    "x64", "x86", "64bit", "32bit", "amd64", "arm64", "win64", "win32",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppInfo {
    pub display_name: String,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub install_location: Option<String>,
    #[serde(default)]
    pub technology: String,
    /// Display names of apps that are still installed; their names are never used for matching.
    #[serde(default)]
    pub other_installed_names: Vec<String>,
    /// Install folders of apps that are still installed; never candidates.
    #[serde(default)]
    pub other_install_locations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Folder,
    File,
    RegistryKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub kind: Kind,
    pub path: String,
    pub confidence: Confidence,
    pub reasons: Vec<String>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub candidates: Vec<Candidate>,
    pub note: Option<String>,
}

pub fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_versionish(token: &str) -> bool {
    let token = token.trim_start_matches(['v', 'V']);
    !token.is_empty()
        && token.chars().any(|c| c.is_ascii_digit())
        && token
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '_' | '+' | ','))
}

fn usable(key: &str) -> bool {
    key.chars().count() >= 4 && !GENERIC_NAMES.contains(&key)
}

pub fn publisher_keys(publisher: Option<&str>) -> Vec<String> {
    let Some(publisher) = publisher else {
        return Vec::new();
    };
    let mut tokens: Vec<&str> = publisher
        .split([' ', ','])
        .filter(|t| !t.is_empty())
        .collect();
    while tokens
        .last()
        .is_some_and(|t| LEGAL_SUFFIXES.contains(&normalize(t).as_str()))
    {
        tokens.pop();
    }
    let mut keys = vec![normalize(&tokens.join(" ")), normalize(publisher)];
    keys.retain(|k| k.chars().count() >= 3 && !GENERIC_NAMES.contains(&k.as_str()));
    keys.dedup();
    keys
}

/// Normalized names an app's own folders and keys are likely to use: the
/// display name without parentheses, versions, or architecture words, and the
/// same name without a leading publisher name.
pub fn product_keys(display_name: &str, publishers: &[String]) -> Vec<String> {
    let mut cleaned = String::new();
    let mut depth = 0u32;
    for c in display_name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => cleaned.push(c),
            _ => {}
        }
    }
    let mut tokens: Vec<&str> = cleaned.split_whitespace().collect();
    while tokens.last().is_some_and(|t| {
        is_versionish(t) || ARCHITECTURE_WORDS.contains(&normalize(t).as_str()) || *t == "-"
    }) {
        tokens.pop();
    }
    let full = normalize(&tokens.join(" "));
    let mut keys = vec![full.clone()];
    for publisher in publishers {
        if let Some(rest) = full.strip_prefix(publisher.as_str()) {
            if rest.chars().count() >= 5 {
                keys.push(rest.to_owned());
            }
        }
    }
    keys.retain(|k| usable(k));
    keys.dedup();
    keys
}

struct Matcher {
    products: Vec<String>,
    publishers: Vec<String>,
    protected: Vec<PathBuf>,
    others: Vec<PathBuf>,
}

impl Matcher {
    fn product(&self, name: &str) -> bool {
        self.products.contains(&normalize(name))
    }

    fn publisher(&self, name: &str) -> bool {
        self.publishers.contains(&normalize(name))
    }

    /// A folder or file that may be removed as a whole.
    fn safe(&self, path: &Path) -> bool {
        if is_reparse_point(path) || DirectoryGuard::parents(path).is_err() {
            return false;
        }
        let Ok(canonical) = fs::canonicalize(path) else {
            return false;
        };
        let path = canonical.as_path();
        is_plain_absolute(path)
            && !is_drive_root(path)
            && !is_reparse_point(path)
            && DirectoryGuard::parents(path).is_ok()
            && known_folder(KnownFolder::Desktop).is_none_or(|root| {
                !is_within(path, &root)
                    || path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
            })
            && ![
                KnownFolder::Documents,
                KnownFolder::Downloads,
                KnownFolder::Music,
                KnownFolder::Pictures,
                KnownFolder::Videos,
            ]
            .into_iter()
            .filter_map(known_folder)
            .any(|root| is_within(path, &root))
            && !path
                .components()
                .any(|c| c.as_os_str().eq_ignore_ascii_case("WindowsApps"))
            && !self.protected.iter().any(|p| is_within(p, path))
            && known_folder(KnownFolder::Windows).is_none_or(|w| !is_within(path, &w))
            && !self
                .others
                .iter()
                .any(|o| is_within(o, path) || is_within(path, o))
    }
}

fn subdirectories(path: &Path) -> Vec<PathBuf> {
    fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Folder size without following links, bounded in time and entries.
fn folder_size(path: &Path) -> Option<u64> {
    let started = Instant::now();
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    let mut visited = 0usize;
    while let Some(folder) = stack.pop() {
        for entry in fs::read_dir(&folder).ok()?.filter_map(|e| e.ok()) {
            visited += 1;
            if visited > 200_000 || started.elapsed().as_secs() > 3 {
                return None;
            }
            let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if metadata.file_type().is_symlink() || is_reparse_point(&entry.path()) {
                continue;
            }
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                total += metadata.len();
            }
        }
    }
    Some(total)
}

#[derive(Default)]
struct Found {
    candidates: Vec<Candidate>,
    seen: HashSet<String>,
}

impl Found {
    fn add(
        &mut self,
        kind: Kind,
        path: String,
        confidence: Confidence,
        reasons: Vec<String>,
        id: String,
    ) {
        if self.seen.insert(id.to_lowercase()) {
            let size_bytes = match kind {
                Kind::Folder => folder_size(Path::new(&path)),
                Kind::File => fs::metadata(&path).ok().map(|m| m.len()),
                Kind::RegistryKey => None,
            };
            self.candidates.push(Candidate {
                id,
                kind,
                path,
                confidence,
                reasons,
                size_bytes,
            });
        }
    }

    fn add_path(&mut self, kind: Kind, path: &Path, confidence: Confidence, reasons: Vec<String>) {
        let text = path.to_string_lossy().into_owned();
        let id = format!(
            "{}:{text}",
            if kind == Kind::Folder {
                "folder"
            } else {
                "file"
            }
        );
        self.add(kind, text, confidence, reasons, id);
    }
}

pub fn scan(app: &AppInfo) -> ScanResult {
    if app.technology == "msix" {
        return ScanResult {
            candidates: Vec::new(),
            note: Some("Windows removes a Store app's data together with the app.".into()),
        };
    }
    let publishers = publisher_keys(app.publisher.as_deref());
    let shared: HashSet<String> = app
        .other_installed_names
        .iter()
        .flat_map(|name| product_keys(name, &publishers))
        .collect();
    let mut products = product_keys(&app.display_name, &publishers);
    products.retain(|k| !shared.contains(k));
    if products.is_empty() {
        return ScanResult {
            candidates: Vec::new(),
            note: Some("This app's name is shared with another installed app or too general to match leftovers safely.".into()),
        };
    }
    let matcher = Matcher {
        products,
        publishers,
        protected: protected_roots(),
        others: app
            .other_install_locations
            .iter()
            .map(PathBuf::from)
            .filter(|p| is_plain_absolute(p))
            .collect(),
    };
    let mut found = Found::default();

    // 1. The install folder the app itself registered.
    if let Some(location) = app
        .install_location
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        let path = PathBuf::from(location.trim_matches('"'));
        if path.is_dir() && matcher.safe(&path) {
            let named = matcher.product(&name_of(&path));
            let under_publisher = path
                .parent()
                .is_some_and(|p| matcher.publisher(&name_of(p)));
            let mut reasons = vec!["Recorded by the app as its install folder".to_owned()];
            if named {
                reasons.push("Folder name matches the app name".into());
            }
            if under_publisher {
                reasons.push("Inside a folder named after the publisher".into());
            }
            // Metadata alone cannot authorize a whole directory of user data.
            if named || under_publisher {
                found.add_path(Kind::Folder, &path, Confidence::High, reasons);
            }
        }
    }

    // 2. Per-app folders in well-known data and program locations.
    let local = known_folder(KnownFolder::LocalAppData);
    let roots: Vec<(Option<PathBuf>, &str, bool)> = vec![
        (
            known_folder(KnownFolder::RoamingAppData),
            "AppData\\Roaming",
            false,
        ),
        (local.clone(), "AppData\\Local", false),
        (
            local.map(|l| l.join("Programs")),
            "your programs folder",
            true,
        ),
        (known_folder(KnownFolder::ProgramData), "ProgramData", false),
        (
            known_folder(KnownFolder::ProgramFiles),
            "Program Files",
            true,
        ),
        (
            known_folder(KnownFolder::ProgramFilesX86),
            "Program Files (x86)",
            true,
        ),
    ];
    for (root, label, _program_root) in roots
        .into_iter()
        .filter_map(|(r, l, p)| r.map(|r| (r, l, p)))
    {
        for folder in subdirectories(&root) {
            let name = name_of(&folder);
            if matcher.product(&name) && matcher.safe(&folder) {
                found.add_path(
                    Kind::Folder,
                    &folder,
                    Confidence::Low,
                    vec![format!("Folder in {label} named after the app")],
                );
            } else if matcher.publisher(&name) {
                for child in subdirectories(&folder) {
                    if matcher.product(&name_of(&child)) && matcher.safe(&child) {
                        found.add_path(
                            Kind::Folder,
                            &child,
                            Confidence::High,
                            vec![
                                format!("Folder in {label} named after the app"),
                                "Inside a folder named after the publisher".into(),
                            ],
                        );
                    }
                }
            }
        }
    }

    // 3. Start menu and desktop shortcuts named after the app.
    let start_menus: Vec<PathBuf> = [KnownFolder::RoamingAppData, KnownFolder::ProgramData]
        .into_iter()
        .filter_map(known_folder)
        .map(|r| r.join(r"Microsoft\Windows\Start Menu\Programs"))
        .collect();
    let mut shortcut_roots = start_menus.clone();
    shortcut_roots.extend(known_folder(KnownFolder::Desktop));
    for root in &shortcut_roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let is_shortcut = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("lnk"));
            if is_shortcut && matcher.product(&stem) && matcher.safe(&path) {
                found.add_path(
                    Kind::File,
                    &path,
                    Confidence::Medium,
                    vec!["Shortcut named after the app".into()],
                );
            } else if is_dir
                && start_menus.iter().any(|m| same_path(m, root))
                && matcher.product(&stem)
                && matcher.safe(&path)
            {
                found.add_path(
                    Kind::Folder,
                    &path,
                    Confidence::Medium,
                    vec!["Start menu folder named after the app".into()],
                );
            }
        }
    }

    // 4. Registry keys under SOFTWARE named after the app.
    for (hive, view) in [
        (Hive::CurrentUser, RegView::Shared),
        (Hive::LocalMachine, RegView::Bits64),
        (Hive::LocalMachine, RegView::Bits32),
    ] {
        for name in registry::subkeys(hive, view, "SOFTWARE") {
            let normalized = normalize(&name);
            if STRUCTURAL_KEYS.contains(&normalized.as_str()) {
                continue;
            }
            if matcher.product(&name) {
                add_registry(
                    &mut found,
                    hive,
                    view,
                    format!(r"SOFTWARE\{name}"),
                    Confidence::Low,
                    vec!["Registry key named after the app; ownership is uncertain".into()],
                );
            } else if matcher.publisher(&name) {
                for child in registry::subkeys(hive, view, &format!(r"SOFTWARE\{name}")) {
                    if matcher.product(&child) {
                        add_registry(
                            &mut found,
                            hive,
                            view,
                            format!(r"SOFTWARE\{name}\{child}"),
                            Confidence::High,
                            vec![
                                "Registry key named after the app".into(),
                                "Under the publisher's registry key".into(),
                            ],
                        );
                    }
                }
            }
        }
    }

    let mut candidates = found.candidates;
    candidates.sort_by(|a, b| {
        b.confidence
            .cmp(&a.confidence)
            .then_with(|| a.path.cmp(&b.path))
    });
    ScanResult {
        candidates,
        note: None,
    }
}

fn add_registry(
    found: &mut Found,
    hive: Hive,
    view: RegView,
    subkey: String,
    confidence: Confidence,
    reasons: Vec<String>,
) {
    let id = format!("reg:{}:{}:{subkey}", hive.tag(), view.tag());
    found.add(
        Kind::RegistryKey,
        registry::display_path(hive, view, &subkey),
        confidence,
        reasons,
        id,
    );
}

fn parse_registry_id(id: &str) -> Option<(Hive, RegView, String)> {
    let mut parts = id.strip_prefix("reg:")?.splitn(3, ':');
    let hive = match parts.next()? {
        "hklm" => Hive::LocalMachine,
        "hkcu" => Hive::CurrentUser,
        _ => return None,
    };
    let view = match parts.next()? {
        "64" => RegView::Bits64,
        "32" => RegView::Bits32,
        "shared" => RegView::Shared,
        _ => return None,
    };
    Some((hive, view, parts.next()?.to_owned()))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoveRequest {
    pub app: AppInfo,
    pub ids: Vec<String>,
    pub journal_directory: String,
    #[serde(default)]
    pub owner_window: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveOutcome {
    pub id: String,
    pub path: String,
    pub outcome: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveResult {
    pub batch_id: String,
    pub removed: usize,
    pub failed: usize,
    pub results: Vec<RemoveOutcome>,
}

pub fn remove(request: &RemoveRequest) -> Result<RemoveResult> {
    let journal = Journal::open(&request.journal_directory)?;
    // Revalidate: only items that a fresh scan still reports may be removed.
    let fresh = scan(&request.app);
    let batch_id = new_batch_id();
    let backups = journal.directory().join("backups").join(&batch_id);
    let mut results = Vec::new();
    let mut journal_items = Vec::new();

    for id in &request.ids {
        let Some(candidate) = fresh.candidates.iter().find(|c| &c.id == id) else {
            results.push(RemoveOutcome {
                id: id.clone(),
                path: id
                    .split_once(':')
                    .map(|(_, p)| p.to_owned())
                    .unwrap_or_default(),
                outcome: "rejected".into(),
                detail: Some("No longer matches the app, so it was left unchanged.".into()),
            });
            continue;
        };
        let (outcome, detail, backup) = match candidate.kind {
            Kind::Folder | Kind::File => {
                match recycle(Path::new(&candidate.path), request.owner_window) {
                    Ok(()) => ("recycled", None, None),
                    Err(reason) => ("failed", Some(reason), None),
                }
            }
            Kind::RegistryKey => remove_registry(candidate, &backups),
        };
        journal_items.push(RemovedItem {
            path: candidate.path.clone(),
            kind: format!("{:?}", candidate.kind).to_lowercase(),
            outcome: outcome.to_owned(),
            detail: detail.clone(),
            backup_path: backup.clone(),
        });
        results.push(RemoveOutcome {
            id: id.clone(),
            path: candidate.path.clone(),
            outcome: outcome.to_owned(),
            detail: detail.or(backup.map(|b| format!("Backup saved to {b}"))),
        });
    }

    journal.append(&Event::LeftoversRemoved {
        batch_id: batch_id.clone(),
        at_unix_ms: unix_millis_now(),
        app_name: request.app.display_name.clone(),
        items: journal_items,
    })?;
    let removed = results
        .iter()
        .filter(|r| r.outcome == "recycled" || r.outcome == "deleted")
        .count();
    Ok(RemoveResult {
        batch_id,
        failed: results.len() - removed,
        removed,
        results,
    })
}

fn remove_registry(
    candidate: &Candidate,
    backups: &Path,
) -> (&'static str, Option<String>, Option<String>) {
    let Some((hive, view, subkey)) = parse_registry_id(&candidate.id) else {
        return ("failed", Some("Invalid registry entry.".into()), None);
    };
    let Some(_text) = registry::export(hive, view, &subkey) else {
        return (
            "failed",
            Some("Couldn't back up the key, so it was left unchanged.".into()),
            None,
        );
    };
    if fs::create_dir_all(backups).is_err() {
        return (
            "failed",
            Some("Couldn't save a backup, so the key was left unchanged.".into()),
            None,
        );
    }
    let file = backups.join(format!(
        "{}-{}-{}.reg",
        hive.tag(),
        view.tag(),
        normalize(&subkey).chars().take(60).collect::<String>()
    ));
    if registry::backup_real(hive, view, &subkey, &file).is_err() {
        return (
            "failed",
            Some("Couldn't save a backup, so the key was left unchanged.".into()),
            None,
        );
    }
    let backup = Some(file.to_string_lossy().into_owned());
    match registry::delete_tree(hive, view, &subkey) {
        Ok(()) => ("deleted", None, backup),
        Err(5) => (
            "failed",
            Some("Needs administrator permission, so it was left unchanged.".into()),
            backup,
        ),
        Err(code) => (
            "failed",
            Some(format!("Windows reported error {code}.")),
            backup,
        ),
    }
}

/// Sends a file or folder to the Recycle Bin. If it can't be recycled (for
/// example, it is too large), Windows asks before deleting it permanently.
fn recycle(path: &Path, owner_window: i64) -> std::result::Result<(), String> {
    let _guard = DirectoryGuard::parents(path)?;
    if !path.exists() {
        return Err("It no longer exists.".into());
    }
    // Refuse the whole target if any descendant cannot be inspected or is a
    // link. The shell must never traverse into another location on our behalf.
    let mut stack = vec![path.to_path_buf()];
    let mut entries = 0usize;
    while let Some(item) = stack.pop() {
        entries += 1;
        if entries > 100_000 {
            return Err("Too many entries to revalidate; preserved for manual review.".into());
        }
        if is_reparse_point(&item) {
            return Err("Contains a link or junction; preserved for manual review.".into());
        }
        let metadata = fs::symlink_metadata(&item)
            .map_err(|_| "A target could not be inspected.".to_owned())?;
        if metadata.is_dir() {
            for entry in
                fs::read_dir(&item).map_err(|_| "A folder could not be inspected.".to_owned())?
            {
                stack.push(
                    entry
                        .map_err(|_| "A folder entry could not be inspected.".to_owned())?
                        .path(),
                );
            }
        }
    }
    let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
    from.extend([0, 0]);
    let owner = owner_window as isize;
    // Shell file operations expect a single-threaded COM apartment.
    let result = std::thread::spawn(move || {
        // SAFETY: balanced with CoUninitialize below.
        let initialized = unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) } >= 0;
        let mut operation = SHFILEOPSTRUCTW {
            hwnd: owner as _,
            wFunc: FO_DELETE,
            pFrom: from.as_ptr(),
            pTo: null(),
            fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_WANTNUKEWARNING) as _,
            fAnyOperationsAborted: 0,
            hNameMappings: null_mut(),
            lpszProgressTitle: null(),
        };
        // SAFETY: `from` is double-NUL-terminated and outlives the call.
        let rc = unsafe { SHFileOperationW(&mut operation) };
        if initialized {
            // SAFETY: matches CoInitializeEx.
            unsafe { CoUninitialize() };
        }
        (rc, operation.fAnyOperationsAborted != 0)
    })
    .join()
    .map_err(|_| "The Recycle Bin operation failed.".to_owned())?;
    match result {
        (_, true) => Err("Cancelled; it was left unchanged.".into()),
        (0, false) if !path.exists() => Ok(()),
        (0, false) => Err("Some files are in use and were left in place.".into()),
        (code, _) => Err(format!(
            "Windows couldn't move it to the Recycle Bin (code {code})."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_keys_drop_versions_architecture_and_publisher_prefix() {
        let publishers = publisher_keys(Some("Adobe Inc."));
        assert_eq!(publishers, vec!["adobe".to_owned(), "adobeinc".to_owned()]);
        assert_eq!(
            product_keys("Adobe After Effects 2024", &publishers),
            vec!["adobeaftereffects", "aftereffects"]
        );
        assert_eq!(
            product_keys("Mullvad VPN 2025.4.0", &[]),
            vec!["mullvadvpn"]
        );
        assert_eq!(product_keys("7-Zip 24.08 (x64)", &[]), vec!["7zip"]);
        assert_eq!(
            product_keys("NVM for Windows v1.1.12", &[]),
            vec!["nvmforwindows"]
        );
        assert!(
            product_keys("Updater", &[]).is_empty(),
            "generic names are never used"
        );
        assert!(
            product_keys("App", &[]).is_empty(),
            "short names are never used"
        );
    }

    #[test]
    fn names_shared_with_installed_apps_are_not_matched() {
        let app = AppInfo {
            display_name: "Adobe After Effects 2024".into(),
            publisher: Some("Adobe Inc.".into()),
            install_location: None,
            technology: "win32".into(),
            other_installed_names: vec!["Adobe After Effects 2025".into()],
            other_install_locations: vec![],
        };
        let result = scan(&app);
        assert!(result.candidates.is_empty());
        assert!(result.note.is_some());
    }

    #[test]
    fn store_apps_have_no_leftovers_to_review() {
        let app = AppInfo {
            display_name: "Some Store App".into(),
            publisher: None,
            install_location: None,
            technology: "msix".into(),
            other_installed_names: vec![],
            other_install_locations: vec![],
        };
        assert!(scan(&app).candidates.is_empty());
    }

    #[test]
    fn protected_roots_and_drive_roots_are_never_candidates() {
        let app = AppInfo {
            display_name: "Program Files Destroyer".into(),
            publisher: None,
            install_location: Some(
                known_folder(KnownFolder::ProgramFiles)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            ),
            technology: "win32".into(),
            other_installed_names: vec![],
            other_install_locations: vec![],
        };
        assert!(scan(&app).candidates.is_empty());
        let drive = AppInfo {
            install_location: Some(r"C:\".into()),
            ..app.clone()
        };
        assert!(scan(&drive).candidates.is_empty());
        let appdata = AppInfo {
            install_location: known_folder(KnownFolder::LocalAppData)
                .map(|p| p.to_string_lossy().into_owned()),
            ..app
        };
        assert!(scan(&appdata).candidates.is_empty());
    }

    #[test]
    fn finds_a_named_appdata_folder_and_registry_key_then_recycles_and_backs_up() {
        let name = format!("CleanupLeftoverTest{}", std::process::id());
        let folder = known_folder(KnownFolder::RoamingAppData)
            .unwrap()
            .join(&name);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("settings.ini"), "x=1").unwrap();
        let key = format!(r"SOFTWARE\{name}");
        {
            use windows_sys::Win32::System::Registry::{
                HKEY_CURRENT_USER, REG_SZ, RegCloseKey, RegCreateKeyW, RegSetValueExW,
            };
            let mut handle = null_mut();
            let path = cleanup_core::wide::to_wide(&key);
            let value: Vec<u16> = "hello".encode_utf16().chain([0]).collect();
            unsafe {
                RegCreateKeyW(HKEY_CURRENT_USER, path.as_ptr(), &mut handle);
                RegSetValueExW(
                    handle,
                    cleanup_core::wide::to_wide("Greeting").as_ptr(),
                    0,
                    REG_SZ,
                    value.as_ptr().cast(),
                    (value.len() * 2) as u32,
                );
                RegCloseKey(handle);
            }
        }
        let app = AppInfo {
            display_name: format!("{name} 1.0"),
            publisher: None,
            install_location: None,
            technology: "win32".into(),
            other_installed_names: vec![],
            other_install_locations: vec![],
        };
        let result = scan(&app);
        let ids: Vec<String> = result.candidates.iter().map(|c| c.id.clone()).collect();
        assert!(ids.iter().any(|i| i.starts_with("folder:")), "{ids:?}");
        assert!(
            ids.iter().any(|i| i.starts_with("reg:hkcu:shared:")),
            "{ids:?}"
        );

        let journal = std::env::temp_dir().join(format!("cleanup-leftovers-journal-{name}"));
        let outcome = remove(&RemoveRequest {
            app: app.clone(),
            ids: ids
                .into_iter()
                .chain(["folder:C:\\Windows".to_owned()])
                .collect(),
            journal_directory: journal.to_string_lossy().into_owned(),
            owner_window: 0,
        })
        .unwrap();
        assert!(!folder.exists(), "folder went to the Recycle Bin");
        assert!(
            !registry::exists(Hive::CurrentUser, RegView::Shared, &key),
            "key deleted: {:?}",
            outcome.results
        );
        let rejected = outcome
            .results
            .iter()
            .find(|r| r.outcome == "rejected")
            .unwrap();
        assert!(rejected.path.contains("Windows"));
        let backup = outcome
            .results
            .iter()
            .find_map(|r| r.detail.as_deref().filter(|d| d.starts_with("Backup")));
        let backup_path = backup.unwrap().trim_start_matches("Backup saved to ");
        let bytes = fs::read(backup_path).unwrap();
        let text = String::from_utf16_lossy(
            &bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        );
        assert!(text.contains("\"Greeting\"=\"hello\""), "{text}");
        let _ = fs::remove_dir_all(journal);
    }
}
