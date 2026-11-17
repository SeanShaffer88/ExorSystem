//! Real-time noise filtering of background telemetry, browser caches and indexer writes.

use crate::events::RawEvent;
use std::collections::HashMap;

/// A match rule. All text matching is case-insensitive with `\` normalised to `/`.
#[derive(Debug, Clone)]
pub enum NoiseRule {
    /// Path/key contains every fragment.
    ContainsAll(Vec<String>),
    /// Path/key starts with prefix.
    Prefix(String),
    /// Originating process image name equals.
    Process(String),
}

/// Filter verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Pass downstream.
    Keep,
    /// Discard, with the reason.
    Drop(String),
}

fn norm(s: &str) -> String {
    s.replace('\\', "/").to_lowercase()
}

/// Stateful filter (burst de-duplication needs state).
#[derive(Debug)]
pub struct NoiseFilter {
    rules: Vec<(NoiseRule, String)>,
    dedupe_window_ms: u64,
    last_seen: HashMap<String, u64>,
}

impl NoiseFilter {
    /// No rules, no de-duplication.
    pub fn empty() -> Self {
        Self {
            rules: Vec::new(),
            dedupe_window_ms: 0,
            last_seen: HashMap::new(),
        }
    }

    /// Built-in rules for common Windows background churn, 250 ms burst de-duplication.
    pub fn with_defaults() -> Self {
        let mut f = Self::empty();
        f.dedupe_window_ms = 250;
        let all = |v: &[&str]| NoiseRule::ContainsAll(v.iter().map(|s| norm(s)).collect());
        f.add_rule(all(&["/user data/", "cache"]), "browser cache (Chromium)");
        f.add_rule(
            all(&["/firefox/profiles/", "cache2"]),
            "browser cache (Firefox)",
        );
        f.add_rule(all(&["/inetcache/"]), "browser cache (WinINet)");
        f.add_rule(
            all(&["/programdata/microsoft/search/"]),
            "Windows Search indexer",
        );
        f.add_rule(
            all(&["/programdata/microsoft/windows defender/"]),
            "Defender scan data",
        );
        f.add_rule(
            all(&["/programdata/microsoft/diagnosis/"]),
            "diagnostic telemetry",
        );
        f.add_rule(all(&["/windows/system32/winevt/logs/"]), "event logs");
        f.add_rule(all(&["/windows/prefetch/"]), "prefetch");
        f.add_rule(all(&["/windows/logs/cbs/"]), "servicing logs");
        f.add_rule(all(&["/$recycle.bin/"]), "recycle bin");
        f.add_rule(
            all(&["/system volume information/"]),
            "system volume information",
        );
        f.add_rule(all(&["/userassist/"]), "shell usage tracking");
        f.add_rule(all(&["/explorer/recentdocs"]), "shell usage tracking");
        f.add_rule(
            all(&["/explorer/shellbags", "/shell/bags"]),
            "shell view state",
        );
        f.add_rule(
            NoiseRule::Prefix("hklm/software/microsoft/windows nt/currentversion/perflib".into()),
            "performance counters",
        );
        for (p, why) in [
            ("searchindexer.exe", "Windows Search indexer"),
            ("searchprotocolhost.exe", "Windows Search indexer"),
            ("msmpeng.exe", "Defender engine"),
            ("compattelrunner.exe", "compatibility telemetry"),
            ("diagtrack.exe", "diagnostic telemetry"),
        ] {
            f.add_rule(NoiseRule::Process(p.into()), why);
        }
        f
    }

    /// Add a rule.
    pub fn add_rule(&mut self, rule: NoiseRule, reason: &str) {
        self.rules.push((rule, reason.to_string()));
    }

    /// Set burst de-duplication window (0 disables).
    pub fn set_dedupe_window_ms(&mut self, ms: u64) {
        self.dedupe_window_ms = ms;
    }

    /// Judge one event.
    pub fn judge(&mut self, ev: &RawEvent) -> Verdict {
        let (target, dedupe_key) = match ev {
            RawEvent::Fs { path, reason, .. } => {
                (norm(path), format!("fs|{reason:?}|{}", norm(path)))
            }
            RawEvent::Registry {
                key,
                value_name,
                op,
                ..
            } => {
                let t = match value_name {
                    Some(v) => format!("{}/{}", norm(key), norm(v)),
                    None => norm(key),
                };
                (t.clone(), format!("reg|{op:?}|{t}"))
            }
        };
        let proc = ev.process().map(str::to_lowercase);
        for (rule, reason) in &self.rules {
            let hit = match rule {
                NoiseRule::ContainsAll(parts) => parts.iter().all(|p| target.contains(p.as_str())),
                NoiseRule::Prefix(p) => target.starts_with(&norm(p)),
                NoiseRule::Process(name) => proc.as_deref() == Some(name.to_lowercase().as_str()),
            };
            if hit {
                return Verdict::Drop(reason.clone());
            }
        }
        if self.dedupe_window_ms > 0 {
            let ts = ev.ts_ms();
            if self.last_seen.len() > 100_000 {
                let cutoff = ts.saturating_sub(self.dedupe_window_ms);
                self.last_seen.retain(|_, t| *t >= cutoff);
            }
            if let Some(prev) = self.last_seen.insert(dedupe_key, ts)
                && ts.saturating_sub(prev) < self.dedupe_window_ms
            {
                return Verdict::Drop("burst duplicate".into());
            }
        }
        Verdict::Keep
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::events::{FsReason, RegOp};

    fn fs(path: &str, proc: &str, ts: u64) -> RawEvent {
        RawEvent::Fs {
            usn: 1,
            path: path.into(),
            reason: FsReason::Create,
            process: Some(proc.into()),
            ts_ms: ts,
        }
    }

    #[test]
    fn drops_known_noise_keeps_installs() {
        let mut f = NoiseFilter::with_defaults();
        assert!(matches!(
            f.judge(&fs(
                "C:\\Users\\u\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache\\f_1",
                "chrome.exe",
                1
            )),
            Verdict::Drop(_)
        ));
        assert!(matches!(
            f.judge(&fs(
                "C:\\ProgramData\\Microsoft\\Search\\Data\\x.edb",
                "x.exe",
                1
            )),
            Verdict::Drop(_)
        ));
        assert!(matches!(
            f.judge(&fs("C:\\Temp\\a.tmp", "SearchIndexer.exe", 1)),
            Verdict::Drop(_)
        ));
        assert_eq!(
            f.judge(&fs("C:\\Program Files\\Foo\\foo.exe", "setup.exe", 1)),
            Verdict::Keep
        );
    }

    #[test]
    fn registry_noise_rules() {
        let mut f = NoiseFilter::with_defaults();
        let ev = RawEvent::Registry {
            key: "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\UserAssist\\{G}\\Count".into(),
            value_name: Some("x".into()),
            op: RegOp::SetValue,
            process: None,
            ts_ms: 1,
        };
        assert!(matches!(f.judge(&ev), Verdict::Drop(_)));
        let ev2 = RawEvent::Registry {
            key: "HKLM\\SOFTWARE\\Foo".into(),
            value_name: None,
            op: RegOp::CreateKey,
            process: None,
            ts_ms: 1,
        };
        assert_eq!(f.judge(&ev2), Verdict::Keep);
    }

    #[test]
    fn burst_duplicates_dropped_within_window_only() {
        let mut f = NoiseFilter::with_defaults();
        let p = "C:\\Program Files\\Foo\\a.dll";
        assert_eq!(f.judge(&fs(p, "s.exe", 1000)), Verdict::Keep);
        assert!(matches!(f.judge(&fs(p, "s.exe", 1100)), Verdict::Drop(_)));
        assert_eq!(f.judge(&fs(p, "s.exe", 1600)), Verdict::Keep);
    }

    #[test]
    fn custom_rule() {
        let mut f = NoiseFilter::empty();
        f.add_rule(NoiseRule::Prefix("C:\\Junk".into()), "junk");
        assert!(matches!(
            f.judge(&fs("c:\\junk\\x", "a.exe", 1)),
            Verdict::Drop(_)
        ));
    }
}
