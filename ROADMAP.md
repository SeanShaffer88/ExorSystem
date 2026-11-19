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

## Next milestones

These are open work, not release dates or claims of current support.

1. **Collect native Windows 10/11 results.** Run the [client checklist](#optional-windows-client-testing) with each exact package. Keep installation and upgrade evidence, launch smoke, native WebView2 behavior, and end-to-end workflow results separate.
2. **Make older Watch sessions easier to find.** Extend the existing Recent comparisons, session picker, and row filters with a searchable, paged session archive. Preserve the distinction between cancelled or incomplete runs, unknown coverage, and completed comparisons with zero changes. Do not infer new observations from old records.
3. **Validate the optional live sources in a lab.** Measure journal gaps, cancellation, registry delivery and disconnect behavior before considering any opt-in integration. This does not block the driverless desktop; see the [separate gates](#driver-work-is-separate).

## Watch an install

Use [Registry monitor → Watch an install](docs/DASHBOARD.md#watch-an-install) for the current steps, scope syntax, and coverage results. Watch only compares selected scopes between two reads; skipped scopes are not empty results and no capture attributes changes to a process.

## Before publishing a Windows build

Pass Linux/Windows CI and the **Windows candidate packages (no publish)** workflow. The [Release workflow](docs/WINDOWS-RELEASE.md#6-tag-and-publish-the-tested-candidate) promotes retained packages from a commit reachable from `main`, without rebuilding or reusing a tag. After publishing, rerun the [published-package verifier](docs/WINDOWS-RELEASE.md#7-verify-the-published-downloads-without-launching-them) with the exact tag. Native client sign-off is optional, not a stable publishing gate.

### v0.3.0 release evidence

The [v0.3.0 evidence record](AUDIT.md#v030-release-evidence) links the exact source, CI, candidate, promotion, and published-asset verification. The static checks **passed**; no installer or desktop ran in that verification.

### Optional Windows client testing

The interactive Windows 10/11 matrix for v0.3.0 is **Not run**. Record the exact tag, package SHA-256, OS build, and outcome in the [evidence template](docs/WINDOWS-RELEASE-EVIDENCE.md). Use a clean disposable VM snapshot per format, not a live user profile.

- [ ] Run the [first-launch observer](docs/WINDOWS-RELEASE.md#8-observe-first-launch-and-exit-on-a-disposable-client) and separately test install/extract, normal and elevated behavior, WebView2, removal, upgrade, and profile location with all three formats where applicable.
- [ ] Verify Watch → History, inventory and uninstall review, file quarantine/restore/purge, and backup/rollback with fixture data. Record actual interruption drills separately from simulations.
- [ ] Verify docking, focus, keyboard navigation, Narrator, high contrast, and 100%/150% mixed-monitor DPI on both clients.

## Driver work is separate

Neither live source is enabled in v0.3.0 desktop Watch. [Native live capture, WDK build/load, a Microsoft-assigned altitude, and signing](docs/LIVE-COLLECTION.md#distribution-gates-open) remain open. Do not elevate the desktop to use a collector.
