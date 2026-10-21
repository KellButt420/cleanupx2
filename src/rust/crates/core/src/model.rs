use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::guid::normalize_braced_guid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallTechnology {
    Win32,
    Msi,
    Msix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallScope {
    User,
    Machine,
}

/// Registry hive that holds an uninstall entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hive {
    LocalMachine,
    CurrentUser,
}

impl Hive {
    pub fn tag(self) -> &'static str {
        match self {
            Hive::LocalMachine => "hklm",
            Hive::CurrentUser => "hkcu",
        }
    }

    pub fn display(self) -> &'static str {
        match self {
            Hive::LocalMachine => "HKEY_LOCAL_MACHINE",
            Hive::CurrentUser => "HKEY_CURRENT_USER",
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "hklm" => Some(Hive::LocalMachine),
            "hkcu" => Some(Hive::CurrentUser),
            _ => None,
        }
    }
}

/// Registry view used to read an entry. `HKCU\Software` is shared between the
/// 32-bit and 64-bit views, so per-user entries use [`RegView::Shared`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegView {
    Bits64,
    Bits32,
    Shared,
}

impl RegView {
    pub fn tag(self) -> &'static str {
        match self {
            RegView::Bits64 => "64",
            RegView::Bits32 => "32",
            RegView::Shared => "shared",
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "64" => Some(RegView::Bits64),
            "32" => Some(RegView::Bits32),
            "shared" => Some(RegView::Shared),
            _ => None,
        }
    }
}

/// Windows Installer context a product is installed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MsiContext {
    UserManaged,
    UserUnmanaged,
    Machine,
}

impl MsiContext {
    pub fn tag(self) -> &'static str {
        match self {
            MsiContext::UserManaged => "userManaged",
            MsiContext::UserUnmanaged => "user",
            MsiContext::Machine => "machine",
        }
    }

    pub fn scope(self) -> InstallScope {
        match self {
            MsiContext::Machine => InstallScope::Machine,
            MsiContext::UserManaged | MsiContext::UserUnmanaged => InstallScope::User,
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "userManaged" => Some(MsiContext::UserManaged),
            "user" => Some(MsiContext::UserUnmanaged),
            "machine" => Some(MsiContext::Machine),
            _ => None,
        }
    }
}

/// Stable identifier for a discovered application.
///
/// The string form is what the frontend stores and sends back. It names where
/// the record came from, never what to execute: the core re-reads everything
/// it needs from that location before acting.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppId {
    /// `reg:{hive}:{view}:{keyName}`
    Registry {
        hive: Hive,
        view: RegView,
        key_name: String,
    },
    /// `msi:{context}:{productCode}`
    Msi {
        context: MsiContext,
        product_code: String,
    },
    /// `msix:{packageFamilyName}`
    Msix { family_name: String },
}

impl AppId {
    pub fn parse(value: &str) -> Result<Self> {
        let invalid = || CoreError::invalid(format!("Unrecognized application id: {value:?}"));
        let (kind, rest) = value.split_once(':').ok_or_else(invalid)?;
        match kind {
            "reg" => {
                let mut parts = rest.splitn(3, ':');
                let hive = parts.next().and_then(Hive::from_tag).ok_or_else(invalid)?;
                let view = parts
                    .next()
                    .and_then(RegView::from_tag)
                    .ok_or_else(invalid)?;
                let key_name = parts.next().ok_or_else(invalid)?;
                let view_matches_hive = match hive {
                    Hive::CurrentUser => view == RegView::Shared,
                    Hive::LocalMachine => view != RegView::Shared,
                };
                if !view_matches_hive || !is_valid_key_name(key_name) {
                    return Err(invalid());
                }
                Ok(AppId::Registry {
                    hive,
                    view,
                    key_name: key_name.to_owned(),
                })
            }
            "msi" => {
                let (context, code) = rest.split_once(':').ok_or_else(invalid)?;
                let context = MsiContext::from_tag(context).ok_or_else(invalid)?;
                let product_code = normalize_braced_guid(code).ok_or_else(invalid)?;
                Ok(AppId::Msi {
                    context,
                    product_code,
                })
            }
            "msix" => {
                if !is_valid_family_name(rest) {
                    return Err(invalid());
                }
                Ok(AppId::Msix {
                    family_name: rest.to_owned(),
                })
            }
            _ => Err(invalid()),
        }
    }
}

impl std::fmt::Display for AppId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppId::Registry {
                hive,
                view,
                key_name,
            } => write!(f, "reg:{}:{}:{key_name}", hive.tag(), view.tag()),
            AppId::Msi {
                context,
                product_code,
            } => write!(f, "msi:{}:{product_code}", context.tag()),
            AppId::Msix { family_name } => write!(f, "msix:{family_name}"),
        }
    }
}

/// Registry key names may contain anything except a backslash; we also refuse
/// control characters and absurd lengths so ids stay printable and bounded.
fn is_valid_key_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.contains('\\')
        && !name.chars().any(char::is_control)
}

/// `Name_PublisherId`, where both parts use the package-name character set.
fn is_valid_family_name(name: &str) -> bool {
    let Some((package, publisher_id)) = name.rsplit_once('_') else {
        return false;
    };
    let allowed = |c: char| c.is_ascii_alphanumeric() || c == '.' || c == '-';
    (3..=50).contains(&package.len())
        && package.chars().all(allowed)
        && publisher_id.len() == 13
        && publisher_id.chars().all(|c| c.is_ascii_alphanumeric())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UninstallMethod {
    /// Removed through Windows Installer using the product code.
    Msi {
        product_code: String,
    },
    /// Removed through the package manager.
    Msix {
        package_full_name: String,
    },
    /// The application's registered uninstaller. `command` is for display
    /// only; the core re-reads and re-validates it before launching anything.
    RegisteredCommand {
        command: String,
    },
    NotAvailable {
        reason: String,
    },
}

/// Normalized application record shared by every discovery source.
/// Missing information stays `None`; nothing is estimated or invented.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRecord {
    pub id: String,
    pub display_name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub technology: InstallTechnology,
    pub scope: InstallScope,
    pub install_location: Option<String>,
    /// `YYYY-MM-DD`, only when the source reports a parseable date.
    pub install_date: Option<String>,
    pub estimated_size_bytes: Option<u64>,
    /// Raw icon reference (registry `DisplayIcon` or package logo path),
    /// resolved lazily by the frontend.
    pub icon_source: Option<String>,
    pub uninstall: UninstallMethod,
    pub registry_key: Option<String>,
    pub msi_product_code: Option<String>,
    pub package_full_name: Option<String>,
    pub package_family_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_round_trip_and_allow_colons_in_key_names() {
        let id = AppId::parse("reg:hklm:32:Vendor:App 1.0").unwrap();
        assert_eq!(
            id,
            AppId::Registry {
                hive: Hive::LocalMachine,
                view: RegView::Bits32,
                key_name: "Vendor:App 1.0".into()
            }
        );
        assert_eq!(id.to_string(), "reg:hklm:32:Vendor:App 1.0");
    }

    #[test]
    fn msi_ids_normalize_product_codes() {
        let id = AppId::parse("msi:machine:{a1b2c3d4-0000-1111-2222-abcdefabcdef}").unwrap();
        assert_eq!(
            id.to_string(),
            "msi:machine:{A1B2C3D4-0000-1111-2222-ABCDEFABCDEF}"
        );
    }

    #[test]
    fn msix_ids_require_a_family_name() {
        assert!(AppId::parse("msix:Contoso.App_8wekyb3d8bbwe").is_ok());
        assert!(AppId::parse("msix:Contoso.App").is_err());
        assert!(AppId::parse("msix:Contoso App_8wekyb3d8bbwe").is_err());
    }

    #[test]
    fn malformed_ids_are_rejected() {
        for bad in [
            "",
            "reg",
            "reg:hkcu:64:Key",
            "reg:hklm:shared:Key",
            "reg:hkcr:64:Key",
            "reg:hklm:64:",
            "reg:hklm:64:a\\b",
            "reg:hklm:64:a\nb",
            "msi:machine:not-a-guid",
            "msi:other:{A1B2C3D4-0000-1111-2222-ABCDEFABCDEF}",
            "exe:C:\\evil.exe",
        ] {
            assert!(AppId::parse(bad).is_err(), "{bad:?}");
        }
    }
}
