//! Installed-application discovery.
//!
//! [`scan`] merges three sources into one list of [`AppRecord`]s:
//! Windows Installer products, uninstall registry entries (64-bit, 32-bit and
//! per-user views), and MSIX packages. Results are emitted in batches so the
//! frontend can render progressively, and the scan stops promptly when the
//! cancellation callback returns `true`.

pub mod arp;
pub mod msi;
pub mod msix;
pub mod registry;

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use cleanup_core::model::{AppId, AppRecord, Hive, InstallTechnology, RegView};
use cleanup_core::{CoreError, Result};
use serde::Serialize;

use crate::arp::SkipReason;
use crate::msix::MsixSkips;

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub include_msix: bool,
    pub batch_size: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            include_msix: true,
            batch_size: 200,
        }
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TechnologyCounts {
    pub win32: usize,
    pub msi: usize,
    pub msix: usize,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySkips {
    pub no_display_name: usize,
    pub system_component: usize,
    pub update_or_patch: usize,
    pub no_uninstall_command: usize,
    pub duplicate_product_code: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceError {
    pub source: &'static str,
    pub error: CoreError,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceTimings {
    pub msi_ms: u64,
    pub registry_ms: u64,
    pub msix_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub records: usize,
    pub by_technology: TechnologyCounts,
    pub registry_entries_read: usize,
    pub registry_skipped: RegistrySkips,
    pub msi_products: usize,
    /// Windows Installer products with no visible uninstall entry (typically
    /// components of a larger bundle). Not listed in v1.
    pub msi_products_hidden: usize,
    pub msix_skipped: MsixSkips,
    pub source_errors: Vec<SourceError>,
    pub timings: SourceTimings,
    pub cancelled: bool,
}

const REGISTRY_SOURCES: [(Hive, RegView, &str); 3] = [
    (Hive::LocalMachine, RegView::Bits64, "registry:hklm64"),
    (Hive::LocalMachine, RegView::Bits32, "registry:hklm32"),
    (Hive::CurrentUser, RegView::Shared, "registry:hkcu"),
];

/// Runs a full inventory scan. A failing source is recorded in the summary and
/// does not prevent the other sources from being listed.
pub fn scan(
    options: &ScanOptions,
    emit: &mut dyn FnMut(Vec<AppRecord>),
    cancel: &dyn Fn() -> bool,
) -> ScanSummary {
    let started = Instant::now();
    let mut summary = ScanSummary::default();
    let batch_size = options.batch_size.clamp(1, 5_000);
    let mut emit_batched = |records: Vec<AppRecord>, summary: &mut ScanSummary| {
        for record in &records {
            match record.technology {
                InstallTechnology::Win32 => summary.by_technology.win32 += 1,
                InstallTechnology::Msi => summary.by_technology.msi += 1,
                InstallTechnology::Msix => summary.by_technology.msix += 1,
            }
        }
        summary.records += records.len();
        let mut records = records.into_iter().peekable();
        while records.peek().is_some() {
            emit(records.by_ref().take(batch_size).collect());
        }
    };

    // 1. Windows Installer products, keyed by product code for merging.
    let phase = Instant::now();
    let msi_products: HashMap<String, msi::MsiProduct> = match msi::enumerate(cancel) {
        Ok(products) => products
            .into_iter()
            .map(|p| (p.product_code.clone(), p))
            .collect(),
        Err(error) => {
            summary.source_errors.push(SourceError {
                source: "msi",
                error,
            });
            HashMap::new()
        }
    };
    summary.msi_products = msi_products.len();
    summary.timings.msi_ms = elapsed_ms(phase);

    // 2. Uninstall registry entries.
    let phase = Instant::now();
    let mut emitted_codes = HashSet::new();
    for (hive, view, source) in REGISTRY_SOURCES {
        if cancel() {
            break;
        }
        let entries = match registry::read_uninstall_entries(hive, view, cancel) {
            Ok(entries) => entries,
            Err(error) => {
                summary.source_errors.push(SourceError { source, error });
                continue;
            }
        };
        summary.registry_entries_read += entries.len();
        let mut records = Vec::with_capacity(entries.len());
        for entry in &entries {
            if let Err(reason) = arp::classify(entry) {
                let skips = &mut summary.registry_skipped;
                match reason {
                    SkipReason::NoDisplayName => skips.no_display_name += 1,
                    SkipReason::SystemComponent => skips.system_component += 1,
                    SkipReason::UpdateOrPatch => skips.update_or_patch += 1,
                    SkipReason::NoUninstallCommand => skips.no_uninstall_command += 1,
                }
                continue;
            }
            let code = arp::msi_product_code(entry);
            if let Some(code) = &code {
                if !emitted_codes.insert(code.clone()) {
                    summary.registry_skipped.duplicate_product_code += 1;
                    continue;
                }
            }
            let product = code.as_ref().and_then(|c| msi_products.get(c));
            records.push(arp::build_record(hive, view, entry, product));
        }
        emit_batched(records, &mut summary);
    }
    summary.msi_products_hidden = msi_products
        .keys()
        .filter(|code| !emitted_codes.contains(*code))
        .count();
    summary.timings.registry_ms = elapsed_ms(phase);

    // 3. MSIX packages.
    if options.include_msix && !cancel() {
        let phase = Instant::now();
        let _apartment = msix::WinRtApartment::enter_mta();
        match msix::enumerate(cancel) {
            Ok((records, skips)) => {
                summary.msix_skipped = skips;
                emit_batched(records, &mut summary);
            }
            Err(error) => summary.source_errors.push(SourceError {
                source: "msix",
                error,
            }),
        }
        summary.timings.msix_ms = elapsed_ms(phase);
    }

    summary.cancelled = cancel();
    summary.timings.total_ms = elapsed_ms(started);
    summary
}

/// Whether the application identified by `id` is still registered. Used to
/// verify removal after an uninstaller finishes.
pub fn is_present(id: &AppId) -> Result<bool> {
    match id {
        AppId::Registry {
            hive,
            view,
            key_name,
        } => registry::uninstall_entry_exists(*hive, *view, key_name),
        AppId::Msi { product_code, .. } => Ok(matches!(
            msi::product_state(product_code),
            msi::ProductState::Installed | msi::ProductState::Advertised
        )),
        AppId::Msix { family_name } => {
            let _apartment = msix::WinRtApartment::enter_mta();
            Ok(msix::find_full_name(family_name)?.is_some())
        }
    }
}

fn elapsed_ms(since: Instant) -> u64 {
    since.elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn live_scan_emits_bounded_batches_with_unique_ids() {
        let mut ids = HashSet::new();
        let mut batches = 0;
        let options = ScanOptions {
            include_msix: true,
            batch_size: 50,
        };
        let summary = scan(
            &options,
            &mut |batch| {
                assert!(!batch.is_empty() && batch.len() <= 50);
                batches += 1;
                for record in batch {
                    assert!(ids.insert(record.id.clone()), "duplicate id {}", record.id);
                    assert!(
                        AppId::parse(&record.id).is_ok(),
                        "unparseable id {}",
                        record.id
                    );
                }
            },
            &|| false,
        );
        assert_eq!(summary.records, ids.len());
        assert!(batches > 0);
        assert!(!summary.cancelled);
    }

    #[test]
    fn cancellation_stops_the_scan() {
        let calls = Cell::new(0);
        let summary = scan(&ScanOptions::default(), &mut |_| {}, &|| {
            calls.set(calls.get() + 1);
            true
        });
        assert!(summary.cancelled);
        assert_eq!(summary.records, 0);
    }
}
