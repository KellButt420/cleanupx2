//! Windows Installer product enumeration through the documented MSI API.

use std::ptr::{null, null_mut};

use cleanup_core::guid::normalize_braced_guid;
use cleanup_core::model::MsiContext;
use cleanup_core::wide::{from_wide, to_wide};
use cleanup_core::{CoreError, Result};
use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
use windows_sys::Win32::System::ApplicationInstallationAndServicing::{
    INSTALLSTATE_ABSENT, INSTALLSTATE_ADVERTISED, INSTALLSTATE_DEFAULT, MSIINSTALLCONTEXT,
    MSIINSTALLCONTEXT_ALL, MSIINSTALLCONTEXT_MACHINE, MSIINSTALLCONTEXT_USERMANAGED,
    MSIINSTALLCONTEXT_USERUNMANAGED, MsiEnumProductsExW, MsiGetProductInfoExW,
    MsiQueryProductStateW,
};

const MAX_PRODUCTS: u32 = 100_000;

#[derive(Debug, Clone)]
pub struct MsiProduct {
    pub product_code: String,
    pub context: MsiContext,
    pub name: Option<String>,
    pub version: Option<String>,
    pub publisher: Option<String>,
    pub install_location: Option<String>,
    /// Raw `YYYYMMDD` value as reported by Windows Installer.
    pub install_date: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductState {
    Installed,
    Advertised,
    /// Installed, but for a different user.
    OtherUser,
    NotInstalled,
}

/// Enumerates products visible to the current user: per-user products of the
/// calling user plus per-machine products.
pub fn enumerate(cancel: &dyn Fn() -> bool) -> Result<Vec<MsiProduct>> {
    let mut products = Vec::new();
    let mut index = 0u32;
    while index < MAX_PRODUCTS && !cancel() {
        let mut code = [0u16; 39];
        let mut context: MSIINSTALLCONTEXT = 0;
        // SAFETY: `code` holds the 38 characters + NUL a product code needs;
        // the SID out parameters are optional and passed as null.
        let rc = unsafe {
            MsiEnumProductsExW(
                null(),
                null(),
                MSIINSTALLCONTEXT_ALL as u32,
                index,
                code.as_mut_ptr(),
                &mut context,
                null_mut(),
                null_mut(),
            )
        };
        match rc {
            ERROR_SUCCESS => {}
            ERROR_NO_MORE_ITEMS => break,
            error => return Err(CoreError::win32("MsiEnumProductsExW", error)),
        }
        index += 1;

        let Some(product_code) = normalize_braced_guid(&from_wide(&code)) else {
            continue;
        };
        let context_tag = match context {
            MSIINSTALLCONTEXT_MACHINE => MsiContext::Machine,
            MSIINSTALLCONTEXT_USERMANAGED => MsiContext::UserManaged,
            MSIINSTALLCONTEXT_USERUNMANAGED => MsiContext::UserUnmanaged,
            _ => continue,
        };
        let code_wide = to_wide(&product_code);
        let property = |name: &str| product_info(&code_wide, context, name);
        products.push(MsiProduct {
            name: property("InstalledProductName"),
            version: property("VersionString"),
            publisher: property("Publisher"),
            install_location: property("InstallLocation"),
            install_date: property("InstallDate"),
            product_code,
            context: context_tag,
        });
    }
    Ok(products)
}

pub fn product_state(product_code: &str) -> ProductState {
    let code = to_wide(product_code);
    // SAFETY: `code` is a NUL-terminated product code.
    match unsafe { MsiQueryProductStateW(code.as_ptr()) } {
        INSTALLSTATE_DEFAULT => ProductState::Installed,
        INSTALLSTATE_ADVERTISED => ProductState::Advertised,
        INSTALLSTATE_ABSENT => ProductState::OtherUser,
        _ => ProductState::NotInstalled,
    }
}

fn product_info(code: &[u16], context: MSIINSTALLCONTEXT, property: &str) -> Option<String> {
    let property = to_wide(property);
    let mut len = 0u32;
    // SAFETY: size query; a null value buffer is permitted with a length pointer.
    let rc = unsafe {
        MsiGetProductInfoExW(
            code.as_ptr(),
            null(),
            context,
            property.as_ptr(),
            null_mut(),
            &mut len,
        )
    };
    if (rc != ERROR_SUCCESS && rc != ERROR_MORE_DATA) || len > 32_768 {
        return None;
    }
    let mut buffer = vec![0u16; len as usize + 1];
    let mut capacity = buffer.len() as u32;
    // SAFETY: `buffer` holds `capacity` units.
    let rc = unsafe {
        MsiGetProductInfoExW(
            code.as_ptr(),
            null(),
            context,
            property.as_ptr(),
            buffer.as_mut_ptr(),
            &mut capacity,
        )
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let value = from_wide(&buffer);
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerates_installed_products_without_error() {
        // Read-only smoke test against the real Windows Installer database.
        let products = enumerate(&|| false).unwrap();
        for product in &products {
            assert!(normalize_braced_guid(&product.product_code).is_some());
        }
    }

    #[test]
    fn unknown_products_report_not_installed() {
        assert_eq!(
            product_state("{00000000-0000-0000-0000-000000000000}"),
            ProductState::NotInstalled
        );
    }
}
