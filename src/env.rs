//! Environment variables, read under the `KAVIRI_` name with the old `LENSA_`
//! spelling as a fallback.
//!
//! The tool was called lensa until the rename, and scripts, CI jobs and shell
//! profiles out there still export the old names. Dropping them would not fail
//! loudly, it would quietly record with the wrong browser or write the
//! telemetry sidecar the user asked to suppress, so every variable is looked up
//! under both names and the new one wins.

use std::ffi::OsString;

/// The variable's value as a `String`, current name first.
///
/// Callers pass the suffix only (`"CHROMIUM"`), so a call site cannot spell one
/// of the two prefixes wrong or forget the fallback exists.
pub fn var(suffix: &str) -> Option<String> {
    std::env::var(format!("KAVIRI_{suffix}"))
        .ok()
        .or_else(|| std::env::var(format!("LENSA_{suffix}")).ok())
}

/// The variable's value as an `OsString`, for the ones that name a path.
pub fn var_os(suffix: &str) -> Option<OsString> {
    std::env::var_os(format!("KAVIRI_{suffix}"))
        .or_else(|| std::env::var_os(format!("LENSA_{suffix}")))
}

/// Whether the variable is set at all, for the ones used as a flag.
pub fn is_set(suffix: &str) -> bool {
    var_os(suffix).is_some()
}
