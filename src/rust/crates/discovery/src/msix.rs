//! MSIX / Store package enumeration through `Windows.Management.Deployment`.

use cleanup_core::model::{AppId, AppRecord, InstallScope, InstallTechnology, UninstallMethod};
use cleanup_core::time::ticks_to_iso_date;
use cleanup_core::{CoreError, Result};
use serde::Serialize;
use windows::ApplicationModel::{Package, PackageSignatureKind};
use windows::Management::Deployment::PackageManager;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
use windows::core::HSTRING;

/// Joins the WinRT multithreaded apartment for the lifetime of the guard.
/// Only used on threads the core owns.
pub struct WinRtApartment(bool);

impl WinRtApartment {
    pub fn enter_mta() -> Self {
        // SAFETY: balanced by RoUninitialize in Drop when it succeeds
        // (including S_FALSE, which also requires a matching uninitialize).
        Self(unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok())
    }
}

impl Drop for WinRtApartment {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: matches the successful RoInitialize above.
            unsafe { RoUninitialize() };
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MsixSkips {
    pub framework: usize,
    pub resource: usize,
    pub system: usize,
    pub unreadable: usize,
}

pub fn winrt_error(context: &'static str) -> impl Fn(windows::core::Error) -> CoreError {
    move |error| CoreError::hresult(context, error.code().0)
}

/// Lists packages registered for the current user that a person would
/// recognize as applications. Frameworks, resource packages, and packages
/// signed as part of Windows are excluded.
pub fn enumerate(cancel: &dyn Fn() -> bool) -> Result<(Vec<AppRecord>, MsixSkips)> {
    let manager = PackageManager::new().map_err(winrt_error("PackageManager"))?;
    let packages = manager
        .FindPackagesByUserSecurityId(&HSTRING::new())
        .map_err(winrt_error("FindPackagesByUserSecurityId"))?;

    let mut records = Vec::new();
    let mut skips = MsixSkips::default();
    for package in packages {
        if cancel() {
            break;
        }
        if package.IsFramework().unwrap_or(false) {
            skips.framework += 1;
        } else if package.IsResourcePackage().unwrap_or(false) {
            skips.resource += 1;
        } else if matches!(package.SignatureKind(), Ok(PackageSignatureKind::System)) {
            skips.system += 1;
        } else {
            match record_for(&package) {
                Ok(record) => records.push(record),
                Err(_) => skips.unreadable += 1,
            }
        }
    }
    Ok((records, skips))
}

/// Full name of the current user's package in `family_name`, if registered.
pub fn find_full_name(family_name: &str) -> Result<Option<String>> {
    let manager = PackageManager::new().map_err(winrt_error("PackageManager"))?;
    let packages = manager
        .FindPackagesByUserSecurityIdPackageFamilyName(&HSTRING::new(), &HSTRING::from(family_name))
        .map_err(winrt_error("FindPackagesByUserSecurityIdPackageFamilyName"))?;
    for package in packages {
        if package.IsFramework().unwrap_or(false) || package.IsResourcePackage().unwrap_or(false) {
            continue;
        }
        let full_name = package
            .Id()
            .and_then(|id| id.FullName())
            .map_err(winrt_error("Package.Id.FullName"))?;
        return Ok(Some(full_name.to_string()));
    }
    Ok(None)
}

fn record_for(package: &Package) -> windows::core::Result<AppRecord> {
    let id = package.Id()?;
    let family_name = id.FamilyName()?.to_string();
    let full_name = id.FullName()?.to_string();
    let version = id.Version()?;

    let non_empty = |value: windows::core::Result<HSTRING>| {
        value
            .ok()
            .map(|s| s.to_string())
            .filter(|s| !s.trim().is_empty())
    };
    let display_name = non_empty(package.DisplayName()).unwrap_or(id.Name()?.to_string());

    Ok(AppRecord {
        id: AppId::Msix {
            family_name: family_name.clone(),
        }
        .to_string(),
        display_name,
        publisher: non_empty(package.PublisherDisplayName()),
        version: Some(format!(
            "{}.{}.{}.{}",
            version.Major, version.Minor, version.Build, version.Revision
        )),
        technology: InstallTechnology::Msix,
        // Packages are registered per user even when staged machine-wide.
        scope: InstallScope::User,
        install_location: non_empty(package.InstalledPath()),
        install_date: package
            .InstalledDate()
            .ok()
            .and_then(|date| ticks_to_iso_date(date.UniversalTime)),
        // Package size is not reported by the platform; computing it would
        // require walking the install directory.
        estimated_size_bytes: None,
        icon_source: package
            .Logo()
            .ok()
            .and_then(|uri| non_empty(uri.AbsoluteUri())),
        uninstall: UninstallMethod::Msix {
            package_full_name: full_name.clone(),
        },
        registry_key: None,
        msi_product_code: None,
        package_full_name: Some(full_name),
        package_family_name: Some(family_name),
    })
}
