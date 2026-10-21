//! Classification and normalization of "Add or Remove Programs" registry
//! entries. Pure functions so the rules are testable without a registry.

use cleanup_core::guid::normalize_braced_guid;
use cleanup_core::model::{
    AppId, AppRecord, Hive, InstallScope, InstallTechnology, RegView, UninstallMethod,
};
use cleanup_core::time::parse_compact_date;
use serde::Serialize;

use crate::msi::MsiProduct;
use crate::registry::{RawUninstallEntry, display_path};

/// Why an entry is not shown, mirroring what Windows Settings hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkipReason {
    NoDisplayName,
    SystemComponent,
    UpdateOrPatch,
    NoUninstallCommand,
}

pub fn classify(entry: &RawUninstallEntry) -> Result<(), SkipReason> {
    if entry.display_name.is_none() {
        return Err(SkipReason::NoDisplayName);
    }
    if entry.system_component == Some(1) {
        return Err(SkipReason::SystemComponent);
    }
    let is_update_release = entry.release_type.as_deref().is_some_and(|kind| {
        let kind = kind.to_ascii_lowercase();
        kind.contains("update") || kind.contains("hotfix")
    });
    if entry.parent_key_name.is_some() || is_update_release {
        return Err(SkipReason::UpdateOrPatch);
    }
    if entry.uninstall_string.is_none() && entry.windows_installer != Some(1) {
        return Err(SkipReason::NoUninstallCommand);
    }
    Ok(())
}

/// Product code for entries Windows Installer manages: the key name is the
/// product code and `WindowsInstaller` is 1.
pub fn msi_product_code(entry: &RawUninstallEntry) -> Option<String> {
    if entry.windows_installer != Some(1) {
        return None;
    }
    normalize_braced_guid(&entry.key_name)
}

/// Builds the normalized record. `msi` is the Windows Installer product whose
/// code matches this entry, if any; records are merged only on an exact
/// product-code match, never by name.
pub fn build_record(
    hive: Hive,
    view: RegView,
    entry: &RawUninstallEntry,
    msi: Option<&MsiProduct>,
) -> AppRecord {
    let registry_key = Some(display_path(hive, view, &entry.key_name));
    let registry_id = AppId::Registry {
        hive,
        view,
        key_name: entry.key_name.clone(),
    }
    .to_string();
    let registry_scope = match hive {
        Hive::LocalMachine => InstallScope::Machine,
        Hive::CurrentUser => InstallScope::User,
    };
    let product_code = msi_product_code(entry);
    let no_remove = (entry.no_remove == Some(1)).then(|| UninstallMethod::NotAvailable {
        reason: "The application's installer does not allow it to be removed.".into(),
    });
    let estimated_size_bytes = entry
        .estimated_size_kb
        .filter(|&kb| kb > 0)
        .map(|kb| u64::from(kb) * 1024);
    let install_date = entry.install_date.as_deref().and_then(parse_compact_date);

    let mut record = AppRecord {
        id: registry_id,
        display_name: entry.display_name.clone().unwrap_or_default(),
        publisher: entry.publisher.clone(),
        version: entry.display_version.clone(),
        technology: InstallTechnology::Win32,
        scope: registry_scope,
        install_location: entry.install_location.clone(),
        install_date,
        estimated_size_bytes,
        icon_source: entry.display_icon.clone(),
        uninstall: UninstallMethod::NotAvailable {
            reason: "No uninstall command is registered.".into(),
        },
        registry_key,
        msi_product_code: product_code.clone(),
        package_full_name: None,
        package_family_name: None,
    };

    match (product_code, msi) {
        (Some(code), Some(product)) => {
            record.id = AppId::Msi {
                context: product.context,
                product_code: code.clone(),
            }
            .to_string();
            record.technology = InstallTechnology::Msi;
            record.scope = product.context.scope();
            if record.display_name.is_empty() {
                record.display_name = product.name.clone().unwrap_or_default();
            }
            record.publisher = record.publisher.or_else(|| product.publisher.clone());
            record.version = record.version.or_else(|| product.version.clone());
            record.install_location = record
                .install_location
                .or_else(|| product.install_location.clone());
            record.install_date = record
                .install_date
                .or_else(|| product.install_date.as_deref().and_then(parse_compact_date));
            record.uninstall = no_remove.unwrap_or(UninstallMethod::Msi { product_code: code });
        }
        (Some(_), None) => {
            record.technology = InstallTechnology::Msi;
            record.uninstall = UninstallMethod::NotAvailable {
                reason: "Windows Installer has no record of this product; its registration may be stale."
                    .into(),
            };
        }
        (None, _) => {
            if let Some(no_remove) = no_remove {
                record.uninstall = no_remove;
            } else if let Some(command) = &entry.uninstall_string {
                record.uninstall = UninstallMethod::RegisteredCommand {
                    command: command.clone(),
                };
            }
        }
    }
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use cleanup_core::model::MsiContext;

    const CODE: &str = "{A1B2C3D4-0000-1111-2222-ABCDEFABCDEF}";

    fn entry(name: &str) -> RawUninstallEntry {
        RawUninstallEntry {
            key_name: name.into(),
            display_name: Some(format!("{name} App")),
            uninstall_string: Some(format!(r#""C:\Program Files\{name}\uninstall.exe""#)),
            ..Default::default()
        }
    }

    fn product() -> MsiProduct {
        MsiProduct {
            product_code: CODE.into(),
            context: MsiContext::Machine,
            name: Some("Installer Name".into()),
            version: Some("2.0.0".into()),
            publisher: Some("Installer Publisher".into()),
            install_location: Some(r"C:\Program Files\Msi".into()),
            install_date: Some("20240115".into()),
        }
    }

    #[test]
    fn hides_entries_windows_settings_hides() {
        let mut no_name = entry("A");
        no_name.display_name = None;
        assert_eq!(classify(&no_name), Err(SkipReason::NoDisplayName));

        let mut system = entry("A");
        system.system_component = Some(1);
        assert_eq!(classify(&system), Err(SkipReason::SystemComponent));

        let mut child = entry("A");
        child.parent_key_name = Some("Parent".into());
        assert_eq!(classify(&child), Err(SkipReason::UpdateOrPatch));

        let mut update = entry("A");
        update.release_type = Some("Security Update".into());
        assert_eq!(classify(&update), Err(SkipReason::UpdateOrPatch));

        let mut no_command = entry("A");
        no_command.uninstall_string = None;
        assert_eq!(classify(&no_command), Err(SkipReason::NoUninstallCommand));

        assert_eq!(classify(&entry("A")), Ok(()));
    }

    #[test]
    fn msi_entries_without_uninstall_strings_are_still_visible() {
        let mut msi = entry(CODE);
        msi.uninstall_string = None;
        msi.windows_installer = Some(1);
        assert_eq!(classify(&msi), Ok(()));
    }

    #[test]
    fn win32_records_keep_registry_identity_and_honest_metadata() {
        let mut raw = entry("Tool");
        raw.install_date = Some("not a date".into());
        raw.estimated_size_kb = Some(0);
        let record = build_record(Hive::CurrentUser, RegView::Shared, &raw, None);
        assert_eq!(record.id, "reg:hkcu:shared:Tool");
        assert_eq!(record.technology, InstallTechnology::Win32);
        assert_eq!(record.scope, InstallScope::User);
        assert_eq!(record.install_date, None);
        assert_eq!(record.estimated_size_bytes, None);
        assert!(matches!(
            record.uninstall,
            UninstallMethod::RegisteredCommand { .. }
        ));
    }

    #[test]
    fn msi_entries_merge_only_on_exact_product_code() {
        let mut raw = entry(&CODE.to_lowercase());
        raw.windows_installer = Some(1);
        raw.display_version = None;
        raw.estimated_size_kb = Some(2048);
        let record = build_record(Hive::LocalMachine, RegView::Bits64, &raw, Some(&product()));
        assert_eq!(record.id, format!("msi:machine:{CODE}"));
        assert_eq!(record.technology, InstallTechnology::Msi);
        assert_eq!(record.display_name, format!("{} App", CODE.to_lowercase()));
        assert_eq!(record.version.as_deref(), Some("2.0.0"));
        assert_eq!(record.install_date.as_deref(), Some("2024-01-15"));
        assert_eq!(record.estimated_size_bytes, Some(2048 * 1024));
        assert_eq!(
            record.uninstall,
            UninstallMethod::Msi {
                product_code: CODE.into()
            }
        );
    }

    #[test]
    fn same_named_entries_are_not_merged() {
        let a = build_record(Hive::LocalMachine, RegView::Bits64, &entry("App"), None);
        let b = build_record(Hive::LocalMachine, RegView::Bits32, &entry("App"), None);
        assert_eq!(a.display_name, b.display_name);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn stale_msi_registrations_are_not_offered_for_removal() {
        let mut raw = entry(CODE);
        raw.windows_installer = Some(1);
        let record = build_record(Hive::LocalMachine, RegView::Bits64, &raw, None);
        assert_eq!(record.technology, InstallTechnology::Msi);
        assert!(record.id.starts_with("reg:"));
        assert!(matches!(
            record.uninstall,
            UninstallMethod::NotAvailable { .. }
        ));
    }

    #[test]
    fn no_remove_entries_are_not_offered_for_removal() {
        let mut raw = entry("Locked");
        raw.no_remove = Some(1);
        let record = build_record(Hive::LocalMachine, RegView::Bits64, &raw, None);
        assert!(matches!(
            record.uninstall,
            UninstallMethod::NotAvailable { .. }
        ));
    }
}
