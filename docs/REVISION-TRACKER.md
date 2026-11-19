# ExorSystem revision tracker

This tracks the 40 suggestions in their original order. The [root roadmap](../ROADMAP.md#next-milestones) sets the implementation order. **Partial** means a foundation exists, not that the requested behavior is complete. **Unreleased** refers to branch work, not the Windows downloads. **Planned** is not evidence of implementation. Interactive v0.3.0 results remain [Not run](../AUDIT.md#v030-release-evidence).

## A. Table-stakes capabilities

| # | Revision | Status and next check |
|---|---|---|
| 1 | Batch uninstall | Unreleased Phase 1 slice: [serial multi-select review](DASHBOARD.md#queue-uninstall-reviews-unreleased), live preview and confirmation per item, exit outcomes, skip and explicit retry. Browser fixtures pass. Native validation, signature binding and reviewed quiet fallback remain open. |
| 2 | Broken-uninstaller recovery | Planned, Phases 1 and 3. Inventory MSI product codes and contexts first. Then evaluate Windows Installer recovery and a directory-only review with snapshot ledger evidence and reversible file moves. Do not delete registration metadata to mimic an uninstall. |
| 3 | Post-uninstall diff | Partial, Phase 1. The existing leftover search is heuristic; Watch does not save a reusable pre-install baseline. Persist the necessary bounded evidence first, then compare only compatible scopes and group results by type/confidence. |
| 4 | Startup / autoruns | Planned, Phases 2 and 3. Inventory Run keys, startup folders, tasks, services and shell extensions with permissions and scope visible. Opt-in changes require recovery and explicit review. |
| 5 | System Restore | Planned, Phase 1 prerequisite. Verify a new restore point and record its sequence before app-managed registry/service changes; fail closed if unavailable. This is not a substitute for a verified backup. |
| 6 | Registry quarantine | Planned, Phase 1 design. Verify selected-key `.reg` exports with a SHA-256 manifest. Explicitly review conflicts before restoring through a scoped, audited workflow. The desktop does not edit those keys today. |
| 7 | Quiet switches | Partial, Phase 1. Registry enumeration knows about registered quiet commands; the wizard uses the interactive invocation. Verify installer identity before recommending known switches, never synthesize flags for unknown commands, and review any interactive fallback. |
| 8 | Vendor signatures | Partial, unreleased. The preview shows an offline Authenticode verdict for the executable (embedded, then Windows catalogs; no revocation lookups): signed, unsigned, untrusted or blocked. The run re-reads the registration and re-checks the signature, and the native command enforces it: unverified needs a second confirmation, blocked (changed after signing, revoked, distrusted, policy) never runs. Applies to the wizard and every queued item. Remaining: bind the reviewed file's identity (hash) to the run, verify the cached MSI package for `msiexec` uninstalls, treat a changed catalog-signed file as blocked rather than unsigned, and close the check-to-start window. A signature is not an approval. |
| 9 | `exor` CLI | Unreleased source alpha: [archive and history](CLI.md) use the same storage and versioned JSON with exit codes. `snapshot`, `scan`, `uninstall`, `export`, and Windows distribution remain planned. |
| 10 | Portable discovery | Partial, Phase 2. Explicit registration/forget is available. Test read-only discovery within selected roots, verify metadata and register only selected entries. |
| 11 | Browser extensions | Planned, lower priority. Inventory first, no browser-data access beyond scoped metadata; require export/recovery and opt-in review before any change. |

## B. Developer differentiators

| # | Revision | Status and next check |
|---|---|---|
| 12 | ETW attribution | Planned, Phase 2 eval. Test kernel process/file/registry provider access and event correlation on unelevated Windows clients. Report loss, stop and fallback visibly; attribution is not ownership. |
| 13 | Developer cleanup | Planned, Phase 2. Read-only inventory of rebuildable outputs, package-manager provenance and IDE plugins before any exact reviewed cleanup. Docker/WSL data is not assumed cache. |
| 14 | PATH and environment | Planned, Phase 2. Preview duplicates and missing toolchain, association, task, service and firewall targets, including unknown/unreadable cases. Export reviewed JSON/PowerShell, no execution of generated scripts. |
| 15 | Reproducible environment export | Planned, Phases 2 and 4. Export draft winget, Chocolatey, Scoop and MSIX metadata and Intune/SCCM detection rules. Validate secret-redacted environment profiles before adding restore. |
| 16 | Dependency impact | Partial, Phase 2. Storage and Relationships show recorded links. Add bounded reverse search and shared-runtime/toolchain views; never imply a complete list or mark absence of links safe. |
| 17 | MSI deep inspection/repair | Planned, Phases 1 and 3. Observe product, context, upgrade/patch and source metadata and orphans read-only; repair/removal need separately reviewed, recoverable operations. |
| 18 | Restart Manager | Partial, unreleased. Before a reviewed cleanup moves files, Apply lists the programs that hold any of them open and stops without consuming the approval; ExorSystem never closes them. If Restart Manager itself fails, Apply continues and locked moves still fail closed. Graceful close and reviewed reboot deferral remain Phase 3, as separate operations with clear persisted outcomes. |
| 19 | Plugin SDK | Planned, Phase 3. Sign manifests and separate read, mutation and network grants. Start with read-only adapters before exposing quarantine or plugins in CI. |

## C. Architecture

| # | Revision | Status and next check |
|---|---|---|
| 20 | Evidence tiers | Documented in the [roadmap](../ROADMAP.md#driver-work-is-separate). Tier 0 snapshots are shipped; Tier 1 ETW is planned, Tier 2 USN/callback is lab-only, and Tier 3 minifilter distribution is gated. |
| 21 | Elevated helper | Planned, gated. Separate signed, nonresident, non-autostart binary; exact scope and consent, allowlisted nonce-bound IPC, same ledger, independent review. Never elevate the desktop. |
| 22 | FTS5 | Planned, Phase 3. Measure bounded search across history, paths and publishers against current literal searches. Preserve privacy, migration, and coverage semantics. |
| 23 | Session retention | Partial, Phase 3. Profile copies and held-file rotation exist; no automatic deletion/compaction of ledger history. Verify opt-in archive readability and evidence continuity first. |
| 24 | Performance benchmarks | Planned, Phase 3. Publish repeatable CI baselines and fixture sizes for snapshots, optional ETW, memory and quarantine. Keep OS and runtime conditions visible. |
| 25 | Fingerprint deduplication | Planned evaluation, Phase 3. Measure storage savings with BLAKE3 reuse without coalescing independent observations or inferring ownership from equal bytes. |
| 26 | Shared core / CLI-first | Partial. Engine/storage are separate from Tauri; CLI shares saved-evidence queries. Move remaining orchestration into typed core APIs with the same review and recovery requirements. |
| 27 | Signing, SBOM, reproducible builds | Planned, Phases 1 and 4. Sign and timestamp the exact tested desktop/installers (helper only if shipped), publish signer certificate fingerprint and SBOM. Validate reproducibility separately; checksums are not publisher trust. |

## D. UX

| # | Revision | Status and next check |
|---|---|---|
| 28 | Simple / Developer | Partial, Phase 2. Docked workspace exists. Design a simple default without removing the explicit vendor confirmation or exact cleanup review. |
| 29 | Risk grouping | Partial, Phase 2. Group ownership candidates, shared/uncertain, and advisory/export-only. Confidence alone never preselects files or authorizes a move. |
| 30 | Timeline and archive | Unreleased searchable Watch archive; Phases 2 and 3 extend to uninstall/cleanup, pins, tags and comparisons with compatible saved scopes. |
| 31 | Drop-to-Watch | Planned, Phase 2. Drop stages a path, not execution. Verify the candidate, obtain consent for scoped baseline, then separately confirm launch. |
| 32 | Explorer/Start and tray | Planned, Phase 3. Explicit entry points stage the same review and run in the same user context. Optional tray can show status and request stop/compare, not grant permission. |
| 33 | HTML report | Partial, Phase 2. Markdown and paged JSON exist; self-contained HTML with before/after evidence, risk, restore, signature results, and embedded JSON is planned. |
| 34 | Accessibility and localization | Partial. Accessibility sign-off is required by the unreleased release gate; actual Narrator, mixed-DPI and high contrast remain unverified. Localization to EN, zh-Hans, ES, DE and JA is planned. |

## E. Trust and release

| # | Revision | Status and next check |
|---|---|---|
| 35 | Native release gate | Unreleased [candidate-bound sign-off](release-evidence/README.md) blocks promotion without all six passing client/package pairs. No client runs or VM automation yet; static CI is insufficient. |
| 36 | Community leftover data | Planned, Phase 3. User-reviewed anonymized patterns/hashes only, opt-in distribution of signed versioned data, local matching, no default telemetry. Never treat a match as cleanup authority. |
| 37 | Fuzzing and interruption | Partial, Phase 3. Parser fixtures and handled write faults are not fuzz coverage or power-loss durability. Add defensive parser fuzzing and isolated fault drills before enabling new mutations. |
| 38 | Independent security review | Planned, prerequisite to privileged/plugin mutation. Review authorization, path and file identity, review-to-use races, reparse handling and signature/IPC trust boundaries. Publish findings and remediation. |
| 39 | Close audit evidence gaps | [Native test plan](NATIVE-VALIDATION.md) assigns target milestones and required evidence. Mark results passed only after actual dated runs. No VM or reviewer assignment is implied. |

## F. Product boundaries

| # | Revision | Status and next check |
|---|---|---|
| 40 | Preserve the safety model | Required across every phase: review before deletion, same-volume reversible file handling, no-overwrite restore, append-only evidence, normal user and no mandatory driver. Keep evidence gaps visible and distinguish advice from authority. |
