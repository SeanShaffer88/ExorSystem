# ExorSystem roadmap

Latest stable: [v0.3.0](https://github.com/SeanShaffer88/ExorSystem/releases/tag/v0.3.0). ExorSystem compares selected snapshots and records evidence for review; it does not monitor every system event or automatically remove everything associated with an installation.

## Shipped on main

| Milestone | What shipped |
|---|---|
| v0.1.0 foundation | File, service, and task observations; policy preview; recoverable quarantine; [first Windows x64 packages](https://github.com/SeanShaffer88/ExorSystem/releases/tag/v0.1.0). |
| v0.2.0 driverless desktop | Per-user unelevated profile, classic/MSIX/portable inventory, opt-in registry snapshots, guided leftover review, a dockable workspace, and promotion of tested candidate artifacts without rebuilding. See the [v0.2.0 notes](docs/releases/v0.2.0.md). |
| v0.3.0 Watch and evidence | Saved scope presets and comparison coverage, bounded capture with cancellation, recent comparisons, dated History filters and JSON export, and the bottom-attached Activity log. [PR #29](https://github.com/SeanShaffer88/ExorSystem/pull/29) · [PR #30](https://github.com/SeanShaffer88/ExorSystem/pull/30) |
| v0.3.0 cleanup and profile safety | Exact single-use cleanup plans, reviewed same-volume file moves, verified profile backups and archive rotation, and separate main/log permissions. Back up the whole profile before the schema v5 upgrade. [PR #31](https://github.com/SeanShaffer88/ExorSystem/pull/31) · [v0.3.0 notes](docs/releases/v0.3.0.md) |
| Packaged Windows verification tooling | [PR #33](https://github.com/SeanShaffer88/ExorSystem/pull/33) added a repeatable read-only check of the **published** NSIS, MSI, portable ZIP, and their evidence files. It also added the optional disposable-client launch observer. The tooling is on `main`; no package was rebuilt or changed for it. |

## Unreleased

**Searchable Watch archive.** Browse recorded sessions beyond Recent comparisons and the recent-session picker. Search by application name or session ID, filter by status, and open the original evidence in Change inspector. Cancelled or incomplete sessions, unavailable historical coverage, and saved zero-change comparisons stay distinct. See the [dashboard guide](docs/DASHBOARD.md#browse-the-watch-archive-unreleased).

**Serial uninstall review queue.** Select supported classic apps across inventory pages, then preview and confirm each vendor invocation. The queue keeps per-item outcomes, skip, and fresh review on retry. No quiet mode, background runs, or automatic cleanup. [How to use it](docs/DASHBOARD.md#queue-uninstall-reviews-unreleased).

## Product direction

Become an evidence-first uninstaller and developer environment manager. Add useful automation without turning a finding, a confidence score, a vendor signature, or a successful exit code into permission to remove anything.

- **Keep the desktop driverless and unelevated.** Snapshots remain the default. If a privileged operation is eventually supported, a separately signed, short-lived helper may perform only the exact reviewed operation, with explicit consent for each operation, narrow allowlisted IPC authenticated with a per-session nonce, and results in the same append-only ledger. Require security review before shipping. No generic elevated command runner, resident or auto-start helper, or fallback elevation.
- **Review, verify, and preserve recovery.** Keep handle-bound same-volume file quarantine and no-overwrite restore. Registry, service, task, and driver findings remain advisory by default. A future registry quarantine must be a separate, narrowly scoped opt-in with verified exports, conflict-aware restore, and an observed System Restore point before any permitted registry/service mutation. Stop if backup, restore point, scope, or identity checks fail. Never infer ownership or grant deletion authority from attribution alone.
- **Keep the ledger append-only.** Record executed mutations, recovery attempts, and outcomes without rewriting observations. Exports, plugin permissions, and community data must preserve local-first defaults, with sensitive values omitted unless deliberately reviewed. Never turn an unobserved or skipped scope into an empty result.

## Next milestones

Phase horizons are planning estimates, not release dates. Unchecked work is not shipped or validated. The [40-item revision tracker](docs/REVISION-TRACKER.md) maps the suggestions to these phases and identifies what is partial, deferred, or awaiting Windows evidence.

### Phase 1 — Uninstall credibility and release trust (0–3 months)

- [x] **Candidate-bound promotion gate (unreleased).** The Release workflow verifies the [reviewed sign-off](docs/release-evidence/README.md) against all six OS/package pairs and includes the matrix in the release notes. It can be tested with fixtures without tagging or publishing.
- [ ] **Native results and VM automation.** Test all three exact candidate packages on Windows 10 and 11 x64, with install/extract, launch, upgrade, removal, UAC/profile behavior, native WebView2 and IPC, Watch → cleanup → restore, and keyboard/Narrator/high-contrast/mixed-DPI checks. Collect the first real sign-off and add disposable VM automation for package and critical UI flows (evaluate a native UI Automation runner such as WinAppDriver and Playwright attached to packaged WebView2; browser mocks are not equivalent). Use the [native audit test plan](docs/NATIVE-VALIDATION.md) for manual gaps and dates. Windows Server, browser mocks, and launch-only evidence cannot pass this gate.
- [ ] **Verified vendor launch and signed releases.** Verify and show Authenticode status before running supported uninstallers; flag unsigned/invalid/unverified files and refuse stale reviewed binaries or invocations. Sign and timestamp the desktop and installers before candidate checks, publish the signer certificate fingerprint, and generate an SPDX or CycloneDX SBOM. Certificate provisioning and candidate signing validation are open; current hashes are not signatures.
- [x] **Serial multi-app review (unreleased).** Multi-select, one live preview and separate confirmation per item, visible exit outcomes, skip, and explicit fresh-preview retry. Keep cleanup separate. Browser fixtures passed; packaged client checks are open.
- [ ] **Remaining batch and quiet-mode work.** Add signature and binary identity binding before considering unattended execution. A later quiet-mode step may recommend known switches for verified MSI/WiX, Inno, NSIS or InstallShield identities, with explicitly reviewed interactive fallback. Never infer silent switches for unknown vendors, auto-retry interrupted work, or queue unattended destructive cleanup.
- [ ] **Recovery and post-uninstall review.** Start with read-only MSI product codes and installation context. Evaluate reviewed Windows Installer recovery via `MsiConfigureProductEx` only after context, package identity, and recovery checks. Design a separate remove-from-directory flow for broken uninstallers using explicit scope, saved snapshot evidence in the ledger, and reversible quarantine—not registration stripping or broad deletion. Add a dedicated diff of the scoped post-uninstall scan against saved pre-install evidence, grouped by artifact type and confidence. Review system findings and never reconstruct pre-install state when a baseline was not saved.
- [ ] **Registry quarantine and restore point prerequisites.** Threat-model the helper and the exact registry transactions before implementing writes. Test timestamped `.reg` exports with hashes in a per-operation backup manifest. Verify the export and confirm `SRSetRestorePoint` actually created a point; record the sequence in the ledger before any permitted registry or service mutation. Test protected-scope refusal, verified conflict-safe restoration, fault recovery, and audit continuity. A failed or disabled restore point must not silently permit a mutation. No blanket service or driver removal.
- [x] **`exor` CLI alpha (source build).** [Read saved Watch sessions and ledger pages](docs/CLI.md) with a versioned JSON contract, explicit database path, exit codes, and no writes or elevation.
- [ ] **CLI distribution and more commands.** Expose bounded `snapshot`, `scan`, `uninstall`, and `export` operations through shared typed core APIs only when review, cancellation, identity, and audit contracts exist. Have Tauri and the CLI call the same core, not launch each other or bypass policy. Validate the Windows build before distribution.

### Phase 2 — Developer workflows (3–6 months)

- [ ] **Deep Watch lab.** Evaluate NT Kernel Logger and kernel process/file/registry ETW providers for opt-in evidence with explicit scopes, permission checks, cancellation, gap/loss reporting, and snapshot fallback. Distinguish event delivery and process correlation from ownership. Treat payload coverage, privileges and overhead as Windows-build-dependent until measured. Validate USN and the registry callback separately; neither is a requirement for ETW or the desktop.
- [ ] **Developer Cleanup.** Inventory explicit project roots for rebuildable artifacts such as `node_modules`, `target/`, `.venv`, `__pycache__`, Gradle/Maven and NuGet/Cargo caches. Inspect global tools by package-manager provenance (npm, pipx, Cargo, .NET, Go, Scoop, Chocolatey, winget) and IDE extensions; cache or folder names alone are not ownership. Inventory Docker volumes/images and WSL distros read-only; treat them as user data, not assumed disposable caches. Introduce scoped, opt-in cleanup only via reviewed actions with supported recovery. Add opt-in portable discovery from selected roots with explicit registration of selected matches; discovery alone never registers or launches anything.
- [ ] **Environment hygiene.** Preview duplicate and missing PATH entries and stale `JAVA_HOME`, `CARGO_HOME`, `GOROOT`, `NODE_PATH`, and `PYTHONPATH` references before considering reviewed deduplication. Report orphan candidates for file associations and protocol handlers (including `HKCU\Software\Classes`), services, drivers, firewall rules and tasks. Start with read-only startup/autorun inventory (Run keys, startup folders, tasks, services and shell extensions) and certificate status. Inventory browser extensions read-only before evaluating separately reviewed cleanup. Export redacted JSON or explicit PowerShell review plans; no automatic execution or broad deletion.
- [ ] **Reproduce and explain.** Export draft winget manifests, Chocolatey/Scoop/MSIX scaffolds, Intune/SCCM detection rules, and secret-redacted developer profiles (JSON/YAML). Require verification of installer type, quiet switches and package IDs; never substitute guesses. Surface reverse dependencies and shared runtime/toolchain impact (redistributables, VS Build Tools, SDKs, LLVM, CMake) with source, confidence and unknown state. Count recorded links as evidence, not an exhaustive list of programs using the component. Add a self-contained HTML report with before/after evidence, risk-grouped diffs, a quarantine manifest and restore instructions, signature status, and embedded machine-readable JSON.
- [ ] **Simpler desktop and evidence timeline.** Make Simple the default for app selection, reviewed uninstall, and a leftover summary. Keep the docked Developer workspace for scopes, evidence, impact and exports. Group review into ownership candidates, shared/uncertain, and advisory/export-only. A strong finding cannot establish ownership, preselect an exact file plan, or approve a move. Extend the Watch archive to a searchable timeline of installs, uninstalls, watches and cleanup, with tags/pins and comparisons only between compatible saved evidence. Prototype drag-and-drop installer staging with signature review and explicit capture/launch consent, not launch-on-drop.

### Phase 3 — Ecosystem and hardening (6–12 months)

- [ ] **Integration surface.** Design a versioned, signed, capability-scoped plugin SDK before admitting third-party plugins. Start with read-only adapters for IDEs, language toolchains, Docker/WSL and CI. Keep network and mutation grants separate and off by default. Add explicitly opted-in and user-reviewed submissions of generalized patterns or publisher hashes, excluding full paths and identifying data. Match a signed, versioned, auditable rules database locally. No default telemetry or mutation authority.
- [ ] **Windows depth.** Expand MSI inspection via `MsiEnumProductsEx` and `MsiGetProductInfoEx` across readable user/machine contexts: product/upgrade codes, patch state, source, version, and orphan diagnosis; add separately approved repair. Use Restart Manager (`RmStartSession` / `RmGetList`) for blocker reporting before considering graceful close or explicitly reviewed reboot deferral, never process termination by default. Add narrowly approved startup changes only after the backup/restore controls. Evaluate optional Explorer/Start actions for Watch and uninstall, and a tray with explicit stop/compare; keep staging and launch review explicit.
- [ ] **Performance and usability.** Benchmark snapshots, optional live capture, memory and quarantine throughput in CI. Evaluate FTS5 for history, paths, and publisher search. Design policy-driven, opt-in session archival that verifies the archive and preserves audit links before any compaction. Measure content-addressed BLAKE3 fingerprint deduplication before changing storage; keep independent observations even if bytes match. Localize the UI (English, Simplified Chinese, Spanish, German and Japanese), retaining native accessibility checks.
- [ ] **Adversarial and recovery tests.** Fuzz USN/callback records, uninstall strings and manifest/import inputs; exercise interruption and power loss across quarantine, cleanup and any new mutation workflow. Publish limits and keep synthetic faults separate from observed durability.

### Phase 4 — Reproducibility and independent assurance (12+ months)

- [ ] Reproducible builds and restorable developer environments; CI/CD-native automation with the same approvals; validated enterprise packaging and Intune/SCCM deployment.
- [ ] Independent published security review of IPC, elevation, launch/signature binding, race and reparse handling, plus formal verification of the quarantine transaction and recovery invariants. Complete the relevant reviews before enabling privileged or third-party mutation surfaces, even if a feature otherwise lands earlier.

## Watch an install

Use [Registry monitor → Watch an install](docs/DASHBOARD.md#watch-an-install) for the current steps, scope syntax, and coverage results. Watch only compares selected scopes between two reads; skipped scopes are not empty results and no capture attributes changes to a process.

## Before publishing a Windows build

The forward release policy is passing Linux/Windows CI, exact retained-candidate verification, and **passing native Windows 10/11 client sign-off** for that candidate. Promote the [same tested bytes](docs/WINDOWS-RELEASE.md#6-tag-and-publish-the-tested-candidate) from a commit reachable from `main`. Do not rebuild, reuse a tag, or use hash checks as evidence that an installer or desktop ran. After publishing, repeat the [published-package verifier](docs/WINDOWS-RELEASE.md#7-verify-the-published-downloads-without-launching-them). Signing and the automated VM harness are separate Phase 1 work.

### v0.3.0 release evidence

The [v0.3.0 evidence record](AUDIT.md#v030-release-evidence) links the exact source, CI, candidate, promotion, and published-asset verification. The static checks **passed**; the interactive Windows 10/11 matrix is **Not run**. The new release policy does not change the already-published packages or backfill results.

### Native Windows client testing

Use the [client runbook](docs/WINDOWS-RELEASE.md#2-install-start-and-remove-each-format) and [evidence template](docs/WINDOWS-RELEASE-EVIDENCE.md) on a disposable VM snapshot per format. Record exact package hashes, build, test outcomes, and redacted evidence in the [candidate sign-off](docs/release-evidence/README.md). Keep the optional first-launch observer separate from full sign-off.

## Driver work is separate

| Evidence tier | Boundary |
|---|---|
| Tier 0 — Default snapshots | Before/after observations of selected scopes. Unelevated; no process attribution. |
| Tier 1 — Opt-in ETW | Driverless trial; correlate process and event data only where supported. Permission denial, gaps and unavailable payloads remain visible. |
| Tier 2 — USN and registry callback lab | USN indicates file journal observations, not process attribution. Callback events and their completeness need independent validation. Neither source is enabled in desktop Watch. |
| Tier 3 — Optional minifilter | Lab/enterprise only, never required. WDK load/unload, altitude assignment, compatibility, signing and independent review are open [distribution gates](docs/LIVE-COLLECTION.md#distribution-gates-open). |

Do not elevate the desktop to use a collector or weaken Windows security controls.
