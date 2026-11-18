# Optional live collection: evaluation and lab checks

**Decision: keep both live sources out of the desktop.** Default Watch uses read-only before/after snapshots and runs without elevation. Neither the release workflow nor the candidate workflow builds, installs, or loads the registry driver. Snapshot evidence is not upgraded to live attribution merely because live parsers exist.

## What the review found

| Source | What is ready | Why it is not ready to ship in Watch |
|---|---|---|
| NTFS USN | A portable V2 parser and an opt-in Windows journal reader; malformed input tests | Requires access to an active NTFS journal. The lab reader reports journal gaps, unsupported records, and queue overflow as incomplete sample results; native discontinuity handling still needs lab verification. Cached parent paths may be stale after rename, and unresolved paths are not authoritative. No process attribution. |
| Registry callback | A portable packet parser and a Windows minifilter port client | No WDK build or load evidence. Delete-key is an attempt, not confirmed removal. The lab client requests cancellation and drains its overlapped receive; actual Windows stop/disconnect needs verification. The lab still needs to verify the port ACL, one-client behavior, shutdown, bounded callback latency and loss reporting. |

Before desktop integration, require explicit opt-in, visible permission/unsupported errors, bounded waits and cancellation, loss/disconnect reporting, scoped capture and a tested fallback to snapshots. Keep observations advisory: a live event must never authorize cleanup. An elevated receiver, if ever required, needs its own separately reviewed trust boundary; do not elevate the desktop to use it.

## Repeatable checks, isolated from the desktop

1. `cargo test -p exor-win --locked` covers parser fixtures and, on Windows, read-only operating-system probes. Run both Windows and Linux CI for changes to the wire layout or timestamps.
2. In a disposable NTFS Windows VM, prepare a new [fixture](WINDOWS-RELEASE.md#3-verify-watch-and-history-with-real-files-and-registry-keys). From a separate lab terminal with permission to read the journal, build and run the short, read-only sample. Use the drive containing the fixture (replace `C` and `<fixture-id>`):

   ```powershell
   cargo run -p exor-win --locked --example live-probe -- usn C <fixture-id>
   ```

   After the sample starts, run fixture `ChangeA` in the fixture terminal. Record create/modify/delete counts and whether the reader closes on an inaccessible or inactive journal. Check rename/path resolution on a disposable fixture file. Do not enable, reset or resize the OS journal to make this test pass. Testing gaps or rollover requires a disposable data volume, not the OS volume.
3. For the driver, use a **separate** WDK VM snapshot and the [driver development instructions](../driver/README.md). If the lab already has a validated test build loaded, use a fresh fixture and `cargo run -p exor-win --locked --example live-probe -- registry <fixture-id>` from the lab console. Check successful create/set operations, delete-attempt semantics, denied non-admin connections, port shutdown/unload, reconnect, load/registration logs and sustained-event behavior. Unload and revert the VM. Do not run this test on an end-user installation.

The probe prints only matching fixture operation counts, for up to 30 seconds. It never writes to the journal, sets values, loads the driver, or shares the desktop database. A count is not proof of complete delivery. Record the source commit, Windows build, filesystem or WDK version, fixture steps, counts, any observed gaps, and logs for every attempted lab run.

## Distribution gates (open)

- [ ] WDK build, INF validation, test load/unload, compatibility, bounded operation and soak tests in a lab.
- [ ] Microsoft-assigned altitude and review of both the callback and minifilter registration/INF. The source uses a development placeholder.
- [ ] Production signing and validation on the supported security configuration. Do not weaken Secure Boot or Memory Integrity on end-user systems.

The source review and probe are available; native live-source captures and the distribution gates have **not** been completed.
