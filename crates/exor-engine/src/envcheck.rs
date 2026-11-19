//! PATH and environment hygiene: find duplicate, empty, relative, unresolved and missing PATH
//! entries, and toolchain variables that point at folders that no longer exist. Read-only; the
//! optional fix script is for the person to review and run themselves.
//!
//! Works on raw registry values (so `%SystemRoot%`-style entries are kept as written), with the
//! expansion and file-system probes injected, so it is testable on any OS.

use serde::Serialize;
use std::collections::HashMap;

/// Where a variable is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvScope {
    /// `HKCU\Environment`.
    User,
    /// `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment`.
    Machine,
}

impl EnvScope {
    fn label(self) -> &'static str {
        match self {
            Self::User => "User",
            Self::Machine => "Machine",
        }
    }
}

/// One stored variable, as written in the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvVar {
    /// Scope.
    pub scope: EnvScope,
    /// Name, as stored.
    pub name: String,
    /// Raw value (not expanded).
    pub value: String,
    /// Stored as `REG_EXPAND_SZ`.
    pub expandable: bool,
}

/// What a path is on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// A folder.
    Dir,
    /// A file.
    File,
    /// Nothing there.
    Missing,
    /// Couldn't tell (access denied, offline share).
    Unknown,
}

/// Variables holding one folder; flagged when the folder is missing.
pub const FOLDER_VARS: &[&str] = &[
    "ANDROID_HOME",
    "ANDROID_NDK_HOME",
    "ANDROID_SDK_ROOT",
    "CARGO_HOME",
    "DOTNET_ROOT",
    "FLUTTER_ROOT",
    "GOROOT",
    "GRADLE_HOME",
    "GRADLE_USER_HOME",
    "JAVA_HOME",
    "JDK_HOME",
    "M2_HOME",
    "MAVEN_HOME",
    "NVM_HOME",
    "NVM_SYMLINK",
    "PYTHONHOME",
    "RUSTUP_HOME",
    "VCPKG_ROOT",
];

/// Variables holding a `;`-separated list of folders.
pub const LIST_VARS: &[&str] = &["Path", "GOPATH", "NODE_PATH", "PYTHONPATH"];

/// Every variable the check reads.
pub fn watched_names() -> Vec<&'static str> {
    LIST_VARS.iter().chain(FOLDER_VARS).copied().collect()
}

/// Kind of problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    /// The same folder appears earlier in this variable.
    Duplicate,
    /// A user PATH entry that machine PATH already has (machine PATH is searched first).
    DuplicateOfMachine,
    /// An empty entry (`;;`).
    Empty,
    /// Not an absolute path: resolved against whatever the current folder is, a hijacking risk.
    Relative,
    /// Still contains `%NAME%` after expansion: the variable it refers to isn't set.
    Unresolved,
    /// The folder doesn't exist.
    Missing,
    /// It names a file, not a folder.
    NotAFolder,
}

/// What the fix script does with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FixAction {
    /// Safe to drop: the script removes it.
    Remove,
    /// Needs a person to decide: the script keeps it and marks it.
    Review,
}

/// One problem found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Scope.
    pub scope: EnvScope,
    /// Variable name.
    pub variable: String,
    /// Entry as written (for list variables) or the whole value.
    pub entry: String,
    /// Entry after expansion.
    pub expanded: String,
    /// Position within a list variable.
    pub index: Option<usize>,
    /// Kind.
    pub kind: FindingKind,
    /// Fix-script action.
    pub action: FixAction,
    /// Plain-language explanation.
    pub detail: String,
}

/// Result of a check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnvReport {
    /// Variables read, `scope:name`.
    pub variables: Vec<String>,
    /// Problems, in variable order.
    pub findings: Vec<Finding>,
    /// Findings the fix script would remove on its own.
    pub removable: usize,
    /// Findings left for review.
    pub to_review: usize,
}

/// Comparison key: lower case, backslashes, no trailing separator, no surrounding quotes.
fn key(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .replace('/', "\\")
        .to_lowercase()
        .trim_end_matches('\\')
        .to_string()
}

fn absolute(path: &str) -> bool {
    let p = path.trim().trim_matches('"').replace('/', "\\");
    let b = p.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\')
        || p.starts_with("\\\\")
}

fn unresolved(expanded: &str) -> bool {
    expanded
        .split('%')
        .skip(1)
        .step_by(2)
        .zip(expanded.split('%').skip(2))
        .any(|(name, _)| !name.is_empty() && !name.contains(['\\', ' ', ';']))
}

/// The machine PATH entries, as comparison keys, for the user-vs-machine duplicate check.
fn machine_path_keys(vars: &[EnvVar], expand: &dyn Fn(&str) -> String) -> Vec<String> {
    vars.iter()
        .filter(|v| v.scope == EnvScope::Machine && v.name.eq_ignore_ascii_case("Path"))
        .flat_map(|v| {
            v.value
                .split(';')
                .map(|e| key(&expand(e)))
                .collect::<Vec<_>>()
        })
        .filter(|k| !k.is_empty())
        .collect()
}

fn finding(
    v: &EnvVar,
    entry: &str,
    expanded: &str,
    index: Option<usize>,
    kind: FindingKind,
) -> Finding {
    let shown = if expanded.is_empty() { entry } else { expanded };
    let (action, detail) = match kind {
        FindingKind::Duplicate => (
            FixAction::Remove,
            format!("{shown} is already listed earlier in {}.", v.name),
        ),
        FindingKind::DuplicateOfMachine => (
            FixAction::Remove,
            format!("{shown} is already in the machine PATH, which Windows searches first."),
        ),
        FindingKind::Empty => (FixAction::Remove, format!("Empty entry in {}.", v.name)),
        FindingKind::Relative => (
            FixAction::Review,
            format!(
                "{shown} is not a full path, so programs resolve it against whatever folder they run in."
            ),
        ),
        FindingKind::Unresolved => (
            FixAction::Review,
            format!("{entry} refers to a variable that isn't set."),
        ),
        FindingKind::Missing => (
            FixAction::Review,
            format!(
                "{shown} doesn't exist. It may belong to an uninstalled program, or to a drive that isn't connected."
            ),
        ),
        FindingKind::NotAFolder => (
            FixAction::Review,
            format!("{shown} is a file, not a folder."),
        ),
    };
    Finding {
        scope: v.scope,
        variable: v.name.clone(),
        entry: entry.to_string(),
        expanded: expanded.to_string(),
        index,
        kind,
        action,
        detail,
    }
}

/// Problems with one path value: relative, unresolved, missing, or a file.
fn path_problem(expanded: &str, probe: &dyn Fn(&str) -> PathKind) -> Option<FindingKind> {
    if unresolved(expanded) {
        return Some(FindingKind::Unresolved);
    }
    if !absolute(expanded) {
        return Some(FindingKind::Relative);
    }
    match probe(expanded.trim().trim_matches('"')) {
        PathKind::Missing => Some(FindingKind::Missing),
        PathKind::File => Some(FindingKind::NotAFolder),
        PathKind::Dir | PathKind::Unknown => None,
    }
}

/// Check the stored variables. `expand` resolves `%NAME%`; `probe` looks at the disk.
pub fn check(
    vars: &[EnvVar],
    expand: &dyn Fn(&str) -> String,
    probe: &dyn Fn(&str) -> PathKind,
) -> EnvReport {
    let machine_path = machine_path_keys(vars, expand);
    let mut findings = Vec::new();
    for v in vars {
        let is_list = LIST_VARS.iter().any(|n| n.eq_ignore_ascii_case(&v.name));
        if is_list {
            let is_user_path = v.scope == EnvScope::User && v.name.eq_ignore_ascii_case("Path");
            let mut seen: HashMap<String, usize> = HashMap::new();
            let entries: Vec<&str> = v.value.split(';').collect();
            let last = entries.len().saturating_sub(1);
            for (i, entry) in entries.iter().enumerate() {
                if entry.trim().is_empty() {
                    // a single trailing ';' is common and harmless
                    if i != last || entries.len() == 1 {
                        findings.push(finding(v, entry, "", Some(i), FindingKind::Empty));
                    }
                    continue;
                }
                let expanded = if v.expandable {
                    expand(entry)
                } else {
                    entry.to_string()
                };
                let k = key(&expanded);
                if seen.insert(k.clone(), i).is_some() {
                    findings.push(finding(
                        v,
                        entry,
                        &expanded,
                        Some(i),
                        FindingKind::Duplicate,
                    ));
                    continue;
                }
                if is_user_path && machine_path.contains(&k) {
                    findings.push(finding(
                        v,
                        entry,
                        &expanded,
                        Some(i),
                        FindingKind::DuplicateOfMachine,
                    ));
                    continue;
                }
                if let Some(kind) = path_problem(&expanded, probe) {
                    findings.push(finding(v, entry, &expanded, Some(i), kind));
                }
            }
        } else if !v.value.trim().is_empty() {
            let expanded = if v.expandable {
                expand(&v.value)
            } else {
                v.value.clone()
            };
            if let Some(kind) = path_problem(&expanded, probe) {
                findings.push(finding(v, &v.value, &expanded, None, kind));
            }
        }
    }
    let removable = findings
        .iter()
        .filter(|f| f.action == FixAction::Remove)
        .count();
    EnvReport {
        variables: vars
            .iter()
            .map(|v| format!("{}:{}", v.scope.label().to_lowercase(), v.name))
            .collect(),
        to_review: findings.len() - removable,
        removable,
        findings,
    }
}

/// PowerShell single-quoted literal.
fn ps(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// One line comment, with anything that could end the comment or the line made harmless.
fn comment(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
}

/// A reviewable PowerShell script that fixes what can be fixed safely. It removes duplicate and
/// empty entries, keeps everything else, and marks each entry that needs review so the person
/// can delete that line. Original values are embedded so `-Undo` puts them back. `generated`
/// is a human-readable timestamp for the header. ExorSystem never runs the script.
pub fn fix_script(vars: &[EnvVar], report: &EnvReport, generated: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "#Requires -Version 5.1\n\
         <#\n\
         ExorSystem environment fix script, generated {generated}.\n\
         Review every line before running it. ExorSystem did not change anything.\n\
         \n\
         What it does: rebuilds the variables below without the duplicate and empty entries.\n\
         Entries marked REVIEW are kept; delete a line to drop that entry.\n\
         Run:    powershell -ExecutionPolicy Bypass -File <this file>\n\
         Undo:   powershell -ExecutionPolicy Bypass -File <this file> -Undo\n\
         Machine variables need an elevated PowerShell; user variables don't.\n\
         Programs that are already running keep their old environment until restarted.\n\
         #>\n\
         param([switch]$Undo)\n\
         $ErrorActionPreference = 'Stop'\n\
         $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)\n\
         \n\
         function Set-StoredVariable([string]$Scope, [string]$Name, [string]$Value, [bool]$Expand) {{\n\
         \x20 if ($Scope -eq 'Machine' -and -not $isAdmin) {{ Write-Warning \"Skipped machine ${{Name}}: run this script from an elevated PowerShell.\"; return }}\n\
         \x20 $root = if ($Scope -eq 'Machine') {{ [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment', $true) }} else {{ [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true) }}\n\
         \x20 try {{\n\
         \x20   # Keep the stored type: PATH entries such as %SystemRoot% only work as REG_EXPAND_SZ.\n\
         \x20   $kind = if ($Expand) {{ [Microsoft.Win32.RegistryValueKind]::ExpandString }} else {{ [Microsoft.Win32.RegistryValueKind]::String }}\n\
         \x20   $root.SetValue($Name, $Value, $kind)\n\
         \x20   Write-Host \"Updated $Scope $Name\"\n\
         \x20 }} finally {{ $root.Close() }}\n\
         }}\n\n"
    ));
    let mut touched = 0;
    for v in vars {
        let mine: Vec<&Finding> = report
            .findings
            .iter()
            .filter(|f| f.scope == v.scope && f.variable == v.name)
            .collect();
        if mine.is_empty() {
            continue;
        }
        if v.value.contains(['\r', '\n']) {
            out.push_str(&format!(
                "# {} {}: left alone, its value contains line breaks.\n\n",
                v.scope.label(),
                comment(&v.name)
            ));
            continue;
        }
        touched += 1;
        let var = format!(
            "{}_{}",
            v.scope.label(),
            v.name.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        );
        out.push_str(&format!(
            "# ---- {} {} ({}) ----\n$original_{var} = {}\n",
            v.scope.label(),
            comment(&v.name),
            if v.expandable {
                "REG_EXPAND_SZ"
            } else {
                "REG_SZ"
            },
            ps(&v.value)
        ));
        let is_list = LIST_VARS.iter().any(|n| n.eq_ignore_ascii_case(&v.name));
        if is_list {
            out.push_str(&format!("$fixed_{var} = @(\n"));
            for (i, entry) in v.value.split(';').enumerate() {
                match mine.iter().find(|f| f.index == Some(i)) {
                    Some(f) if f.action == FixAction::Remove => {
                        if !entry.trim().is_empty() {
                            out.push_str(&format!(
                                "  # removed: {}  ({})\n",
                                comment(entry),
                                comment(&f.detail)
                            ));
                        }
                    }
                    Some(f) => out.push_str(&format!(
                        "  {}  # REVIEW: {}\n",
                        ps(entry),
                        comment(&f.detail)
                    )),
                    None if entry.trim().is_empty() => {}
                    None => out.push_str(&format!("  {}\n", ps(entry))),
                }
            }
            out.push_str(&format!(
                ") -join ';'\n\
                 if ($Undo) {{ Set-StoredVariable {s} {n} $original_{var} ${e} }} else {{ Set-StoredVariable {s} {n} $fixed_{var} ${e} }}\n\n",
                s = ps(v.scope.label()),
                n = ps(&v.name),
                e = if v.expandable { "true" } else { "false" },
            ));
        } else {
            let why = mine.first().map(|f| comment(&f.detail)).unwrap_or_default();
            out.push_str(&format!(
                "# REVIEW: {why}\n\
                 # To remove it, uncomment the next line. -Undo puts the original back.\n\
                 # if (-not $Undo) {{ Set-StoredVariable {s} {n} '' ${e}; Write-Host 'Cleared; delete the empty value later if you like.' }}\n\
                 if ($Undo) {{ Set-StoredVariable {s} {n} $original_{var} ${e} }}\n\n",
                s = ps(v.scope.label()),
                n = ps(&v.name),
                e = if v.expandable { "true" } else { "false" },
            ));
        }
    }
    if touched == 0 {
        out.push_str("Write-Host 'Nothing to change.'\n");
    } else {
        out.push_str(
            "# Tell Explorer and new programs that the environment changed.\n\
             Add-Type -Namespace ExorSystem -Name Native -MemberDefinition '[DllImport(\"user32.dll\", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);'\n\
             $result = [UIntPtr]::Zero\n\
             [void][ExorSystem.Native]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)\n\
             Write-Host 'Done. Open a new terminal to see the change.'\n",
        );
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn var(scope: EnvScope, name: &str, value: &str) -> EnvVar {
        EnvVar {
            scope,
            name: name.into(),
            value: value.into(),
            expandable: true,
        }
    }

    fn expand(s: &str) -> String {
        s.replace("%SystemRoot%", r"C:\Windows")
            .replace("%USERPROFILE%", r"C:\Users\sam")
    }

    fn probe(p: &str) -> PathKind {
        match key(p).as_str() {
            r"c:\windows"
            | r"c:\windows\system32"
            | r"c:\tools"
            | r"c:\users\sam\.cargo\bin"
            | r"c:\java\jdk-21" => PathKind::Dir,
            r"c:\tools\tool.exe" => PathKind::File,
            r"\\server\share" => PathKind::Unknown,
            _ => PathKind::Missing,
        }
    }

    fn kinds(r: &EnvReport) -> Vec<(String, FindingKind)> {
        r.findings
            .iter()
            .map(|f| (f.entry.clone(), f.kind))
            .collect()
    }

    #[test]
    fn finds_path_problems_and_keeps_good_entries() {
        let vars = [
            var(
                EnvScope::Machine,
                "Path",
                r"%SystemRoot%\system32;%SystemRoot%;C:\Tools",
            ),
            var(
                EnvScope::User,
                "Path",
                r"%USERPROFILE%\.cargo\bin;C:\Users\sam\.cargo\bin\;;bin;C:\Tools;D:\Gone;%NOPE%\x;C:\Tools\tool.exe;\\server\share;",
            ),
        ];
        let r = check(&vars, &expand, &probe);
        assert_eq!(
            kinds(&r),
            vec![
                (r"C:\Users\sam\.cargo\bin\".into(), FindingKind::Duplicate),
                ("".into(), FindingKind::Empty),
                ("bin".into(), FindingKind::Relative),
                (r"C:\Tools".into(), FindingKind::DuplicateOfMachine),
                (r"D:\Gone".into(), FindingKind::Missing),
                (r"%NOPE%\x".into(), FindingKind::Unresolved),
                (r"C:\Tools\tool.exe".into(), FindingKind::NotAFolder),
            ]
        );
        assert_eq!((r.removable, r.to_review), (3, 4));
        assert!(r.findings.iter().all(|f| f.scope == EnvScope::User));
    }

    #[test]
    fn folder_variables_are_checked_and_unset_ones_ignored() {
        let vars = [
            var(EnvScope::User, "JAVA_HOME", r"C:\Java\jdk-21"),
            var(EnvScope::User, "GOROOT", r"C:\Go"),
            var(EnvScope::Machine, "ANDROID_HOME", ""),
        ];
        let r = check(&vars, &expand, &probe);
        assert_eq!(kinds(&r), vec![(r"C:\Go".into(), FindingKind::Missing)]);
        assert_eq!(r.findings[0].index, None);
    }

    #[test]
    fn unresolved_detection_ignores_literal_percent_signs() {
        assert!(unresolved(r"%NOPE%\bin"));
        assert!(!unresolved(r"C:\100% done\bin"));
        assert!(!unresolved(r"C:\Tools"));
    }

    #[test]
    fn fix_script_removes_only_safe_entries_keeps_review_items_and_can_undo() {
        let vars = [
            var(EnvScope::Machine, "Path", r"%SystemRoot%\system32"),
            var(
                EnvScope::User,
                "Path",
                r"C:\Tools;c:\tools\;;D:\Gone;O'Brien\bin",
            ),
            var(EnvScope::User, "GOROOT", r"C:\Go"),
            var(EnvScope::User, "JAVA_HOME", r"C:\Java\jdk-21"),
        ];
        let r = check(&vars, &expand, &probe);
        let s = fix_script(&vars, &r, "2026-11-19 10:00");
        assert!(
            s.contains("$original_User_Path = 'C:\\Tools;c:\\tools\\;;D:\\Gone;O''Brien\\bin'"),
            "{s}"
        );
        // kept, in order; duplicate and empty dropped; review items marked
        let fixed = s
            .split("$fixed_User_Path = @(")
            .nth(1)
            .unwrap()
            .split(") -join")
            .next()
            .unwrap();
        assert!(fixed.contains("  'C:\\Tools'\n"), "{fixed}");
        assert!(fixed.contains("# removed: c:\\tools\\"), "{fixed}");
        assert!(fixed.contains("'D:\\Gone'  # REVIEW:"), "{fixed}");
        assert!(fixed.contains("'O''Brien\\bin'  # REVIEW:"), "{fixed}");
        // machine Path had no findings, JAVA_HOME is fine: neither is touched
        assert!(!s.contains("Machine Path"));
        assert!(!s.contains("JAVA_HOME"));
        // folder variables are never changed unless the person uncomments the line
        assert!(s.contains("# if (-not $Undo) { Set-StoredVariable 'User' 'GOROOT' ''"));
        assert!(s.contains("[Microsoft.Win32.RegistryValueKind]::ExpandString"));
        assert!(
            s.contains("if ($Undo) { Set-StoredVariable 'User' 'Path' $original_User_Path $true }")
        );
    }

    /// Runs a generated script for real, but pointed at a throwaway `HKCU\Software` key instead of
    /// `HKCU\Environment`, so the person's environment is never touched.
    #[cfg(windows)]
    #[test]
    fn generated_script_fixes_keeps_the_value_type_and_undoes() {
        use std::process::Command;
        let vars = [var(
            EnvScope::User,
            "Path",
            r"%SystemRoot%;C:\Windows;;X:\gone\O'Brien",
        )];
        let r = check(&vars, &expand, &probe);
        let test_key = format!("Software\\ExorSystemTest-{}", std::process::id());
        let script = fix_script(&vars, &r, "test").replace(
            "[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)",
            &format!("[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('{test_key}')"),
        );
        assert!(
            script.contains(&test_key),
            "the registry target was not redirected"
        );
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("fix.ps1");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(script.replace('\n', "\r\n").as_bytes());
        std::fs::write(&file, bytes).unwrap();
        let ps = |args: &[&str]| {
            let out = Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        let read = format!(
            "$k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('{test_key}'); \
             $k.GetValueKind('Path').ToString() + '|' + $k.GetValue('Path', $null, 'DoNotExpandEnvironmentNames'); $k.Close()"
        );
        let cleanup = format!(
            "[Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree('{test_key}', $false)"
        );
        let result = std::panic::catch_unwind(|| {
            ps(&["-File", file.to_str().unwrap()]);
            let fixed = ps(&["-Command", &read]);
            ps(&["-File", file.to_str().unwrap(), "-Undo"]);
            let undone = ps(&["-Command", &read]);
            (fixed, undone)
        });
        ps(&["-Command", &cleanup]);
        let (fixed, undone) = result.unwrap();
        // duplicate (C:\Windows = %SystemRoot%) and empty entries go; the missing one stays for
        // review; %SystemRoot% is kept unexpanded as REG_EXPAND_SZ
        assert_eq!(fixed, r"ExpandString|%SystemRoot%;X:\gone\O'Brien");
        assert_eq!(
            undone,
            r"ExpandString|%SystemRoot%;C:\Windows;;X:\gone\O'Brien"
        );
    }

    #[test]
    fn clean_environment_produces_a_no_op_script() {
        let vars = [var(EnvScope::Machine, "Path", r"%SystemRoot%\system32")];
        let r = check(&vars, &expand, &probe);
        assert!(r.findings.is_empty());
        assert!(fix_script(&vars, &r, "now").contains("Nothing to change."));
    }
}
