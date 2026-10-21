//! Shared model, identifiers, and error types for the Cleanup native core.
//!
//! Everything in this crate is plain data and pure logic so it can be unit
//! tested without touching the registry, Windows Installer, or the package
//! manager.

pub mod error;
pub mod guid;
pub mod model;
pub mod paths;
pub mod time;
pub mod wide;

pub use error::{CoreError, ErrorCode, Result};
