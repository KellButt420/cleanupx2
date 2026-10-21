//! Parsing and validation of registered uninstall commands.
//!
//! `UninstallString` values are untrusted input. They are never handed to a
//! shell or to `CreateProcess` without an explicit application name. The
//! parser extracts one executable path and its argument string, applies the
//! safety rules below, and otherwise rejects the command so the user can fall
//! back to Windows Settings.

use cleanup_core::guid::normalize_braced_guid;
use cleanup_core::{CoreError, Result};

const MAX_COMMAND_LEN: usize = 4096;
const MAX_PATH_LEN: usize = 1024;

/// Executables that interpret arbitrary scripts or commands. A registered
/// uninstall command that starts one of these is not launched automatically.
const BLOCKED_HOSTS: &[&str] = &[
    "bash.exe",
    "bitsadmin.exe",
    "certutil.exe",
    "cmd.exe",
    "conhost.exe",
    "cscript.exe",
    "curl.exe",
    "explorer.exe",
    "forfiles.exe",
    "hh.exe",
    "installutil.exe",
    "msbuild.exe",
    "msdt.exe",
    "mshta.exe",
    "powershell.exe",
    "powershell_ise.exe",
    "pwsh.exe",
    "reg.exe",
    "regasm.exe",
    "regsvcs.exe",
    "regsvr32.exe",
    "rundll32.exe",
    "sc.exe",
    "schtasks.exe",
    "wmic.exe",
    "wscript.exe",
    "wsl.exe",
    "wt.exe",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedCommand {
    /// A registered uninstaller executable and its own arguments.
    Executable { path: String, arguments: String },
    /// `msiexec` with a product code. Only the product code is kept; the core
    /// always runs `msiexec /x {code}` and never forwards other switches.
    WindowsInstaller { product_code: String },
}

pub fn parse_registered_command(raw: &str) -> Result<ParsedCommand> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(CoreError::rejected(
            "The registered uninstall command is empty.",
        ));
    }
    if raw.len() > MAX_COMMAND_LEN {
        return Err(CoreError::rejected(
            "The registered uninstall command is too long.",
        ));
    }
    if raw.chars().any(char::is_control) {
        return Err(CoreError::rejected(
            "The registered uninstall command contains control characters.",
        ));
    }

    let (path, arguments) = split_executable(raw)?;
    let file_name = path
        .rsplit('\\')
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase();

    // Registered MSI commands are usually a bare "MsiExec.exe /I{code}". The
    // given path is never executed: the core always runs System32\msiexec.exe
    // with "/x" and the validated product code.
    if file_name == "msiexec.exe" || file_name == "msiexec" {
        return msiexec_product_code(arguments)
            .map(|product_code| ParsedCommand::WindowsInstaller { product_code });
    }
    validate_executable_path(path)?;
    if BLOCKED_HOSTS.contains(&file_name.as_str()) {
        return Err(CoreError::rejected(format!(
            "Uninstall commands that run {file_name} are not launched automatically."
        )));
    }
    Ok(ParsedCommand::Executable {
        path: path.to_owned(),
        arguments: arguments.trim().to_owned(),
    })
}

/// Splits `raw` into the executable path and the remaining argument text.
fn split_executable(raw: &str) -> Result<(&str, &str)> {
    if let Some(rest) = raw.strip_prefix('"') {
        let end = rest
            .find('"')
            .ok_or_else(|| CoreError::rejected("The uninstall command has unbalanced quotes."))?;
        let after = &rest[end + 1..];
        if !after.is_empty() && !after.starts_with(char::is_whitespace) {
            return Err(CoreError::rejected(
                "The uninstall command has unexpected text after the quoted path.",
            ));
        }
        return Ok((&rest[..end], after));
    }

    // Unquoted: the executable ends at the first ".exe" followed by
    // whitespace or the end of the string. Unlike CreateProcess's search for
    // unquoted paths, intermediate prefixes such as "C:\Program" are never
    // probed or executed.
    let lower = raw.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("msiexec") {
        if rest.is_empty() || rest.starts_with(char::is_whitespace) {
            return Ok((&raw[..7], &raw[7..]));
        }
    }
    let mut from = 0;
    while let Some(found) = lower[from..].find(".exe") {
        let end = from + found + 4;
        if end == raw.len() || raw[end..].starts_with(char::is_whitespace) {
            return Ok((&raw[..end], &raw[end..]));
        }
        from = end;
    }
    Err(CoreError::rejected(
        "The registered uninstall command does not name an .exe file.",
    ))
}

/// Accepts only local, drive-absolute paths to an `.exe` without relative
/// components, alternate data streams, wildcards, or device/UNC prefixes.
pub fn validate_executable_path(path: &str) -> Result<()> {
    let reject = |why: &str| {
        Err(CoreError::rejected(format!(
            "Uninstaller path {path:?} {why}."
        )))
    };
    let bytes = path.as_bytes();
    if path.len() > MAX_PATH_LEN {
        return reject("is too long");
    }
    if bytes.len() < 4 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return reject("is not an absolute local path");
    }
    if path[2..].contains(':') {
        return reject("contains an alternate data stream or extra drive separator");
    }
    if path.contains(['/', '"', '<', '>', '|', '?', '*']) {
        return reject("contains characters that are not allowed");
    }
    if path[3..]
        .split('\\')
        .any(|part| part.is_empty() || part == "." || part == ".." || part.ends_with([' ', '.']))
    {
        return reject("contains empty, relative, or trailing-dot components");
    }
    if !path.to_ascii_lowercase().ends_with(".exe") {
        return reject("is not an .exe file");
    }
    Ok(())
}

/// Finds the product code in `msiexec` arguments such as `/X{GUID}`,
/// `/I {GUID}`, or `-x{GUID}`.
fn msiexec_product_code(arguments: &str) -> Result<String> {
    let tokens: Vec<&str> = arguments.split_whitespace().collect();
    for (index, token) in tokens.iter().enumerate() {
        let Some(switch) = token.strip_prefix(['/', '-']) else {
            continue;
        };
        let Some(first) = switch.chars().next() else {
            continue;
        };
        if !matches!(first.to_ascii_lowercase(), 'x' | 'i') {
            continue;
        }
        let attached = &switch[1..];
        let candidate = if attached.is_empty() {
            tokens.get(index + 1).copied().unwrap_or_default()
        } else {
            attached
        };
        if let Some(code) = normalize_braced_guid(candidate) {
            return Ok(code);
        }
    }
    Err(CoreError::rejected(
        "The msiexec uninstall command does not contain a valid product code.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cleanup_core::ErrorCode;

    fn exe(path: &str, arguments: &str) -> ParsedCommand {
        ParsedCommand::Executable {
            path: path.into(),
            arguments: arguments.into(),
        }
    }

    fn rejected(raw: &str) -> bool {
        matches!(parse_registered_command(raw), Err(e) if e.code == ErrorCode::Rejected)
    }

    #[test]
    fn parses_quoted_paths_with_arguments() {
        assert_eq!(
            parse_registered_command(r#""C:\Program Files\App\unins000.exe" /SILENT"#).unwrap(),
            exe(r"C:\Program Files\App\unins000.exe", "/SILENT")
        );
        assert_eq!(
            parse_registered_command(r#"  "C:\App\u.exe"  "#).unwrap(),
            exe(r"C:\App\u.exe", "")
        );
    }

    #[test]
    fn unquoted_paths_end_at_the_first_exe_boundary() {
        assert_eq!(
            parse_registered_command(r"C:\Program Files\App\uninstall.exe /x --force").unwrap(),
            exe(r"C:\Program Files\App\uninstall.exe", "/x --force")
        );
        // ".exe" inside a directory name is not a boundary.
        assert_eq!(
            parse_registered_command(r"C:\Tools\a.exes\remove.exe").unwrap(),
            exe(r"C:\Tools\a.exes\remove.exe", "")
        );
    }

    #[test]
    fn msiexec_commands_become_product_code_requests() {
        let expected = ParsedCommand::WindowsInstaller {
            product_code: "{A1B2C3D4-0000-1111-2222-ABCDEFABCDEF}".into(),
        };
        for raw in [
            r"MsiExec.exe /I{a1b2c3d4-0000-1111-2222-abcdefabcdef}",
            r"MsiExec /X{A1B2C3D4-0000-1111-2222-ABCDEFABCDEF}",
            r"C:\Windows\System32\msiexec.exe /X{A1B2C3D4-0000-1111-2222-ABCDEFABCDEF}",
            r#""C:\Windows\System32\MsiExec.exe" /qb /x {A1B2C3D4-0000-1111-2222-ABCDEFABCDEF}"#,
            r"C:\Windows\System32\msiexec.exe -x{A1B2C3D4-0000-1111-2222-ABCDEFABCDEF} TRANSFORMS=evil.mst",
            // The path is never executed, so a planted msiexec.exe is harmless.
            r"C:\Users\Public\msiexec.exe /x{A1B2C3D4-0000-1111-2222-ABCDEFABCDEF}",
        ] {
            assert_eq!(parse_registered_command(raw).unwrap(), expected, "{raw}");
        }
        assert!(rejected(r"C:\Windows\System32\msiexec.exe /x not-a-guid"));
        assert!(rejected(r"C:\Windows\System32\msiexec.exe /qn"));
        assert!(rejected(
            r"msiexecute.exe /x{A1B2C3D4-0000-1111-2222-ABCDEFABCDEF}"
        ));
    }

    #[test]
    fn script_hosts_and_lolbins_are_rejected() {
        for raw in [
            r#"C:\Windows\System32\cmd.exe /c del C:\*"#,
            r#""C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" -enc AAAA"#,
            r"C:\Windows\System32\rundll32.exe shell32.dll,Control_RunDLL",
            r"C:\Windows\System32\MSHTA.EXE http://example.invalid/x.hta",
            r"C:\Windows\System32\wscript.exe C:\x.vbs",
        ] {
            assert!(rejected(raw), "{raw}");
        }
    }

    #[test]
    fn unsafe_paths_are_rejected() {
        for raw in [
            "",
            "   ",
            r"uninstall.exe",
            r"\\server\share\uninstall.exe",
            r"\\?\C:\App\uninstall.exe",
            r"\\.\C:\App\uninstall.exe",
            r"C:App\uninstall.exe",
            r"C:\App\..\Windows\uninstall.exe",
            r"C:\App\.\uninstall.exe",
            r"C:\App\\uninstall.exe",
            r"C:\App\uninstall.exe:stream",
            r"C:\App\file.txt:evil.exe",
            r"C:/App/uninstall.exe",
            r"C:\App\un*.exe",
            r"C:\App\uninstall.bat",
            r#""C:\App\uninstall.exe"/S"#,
            r#""C:\App\uninstall.exe"#,
            "C:\\App\\uninstall.exe\n/S",
            r"C:\App.\uninstall.exe",
        ] {
            assert!(parse_registered_command(raw).is_err(), "{raw:?}");
        }
    }

    #[test]
    fn oversized_commands_are_rejected() {
        let long = format!(r"C:\{}\u.exe", "a".repeat(5000));
        assert!(rejected(&long));
    }
}
