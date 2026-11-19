# ExorSystem documentation

The [project README](../README.md) explains what ExorSystem does and where to download it. The latest stable release is [v0.3.0](https://github.com/SeanShaffer88/ExorSystem/releases/tag/v0.3.0); the docs also cover tooling on `main` that does not change those packages.

## Use the desktop

- [Dashboard guide](DASHBOARD.md): arrange docks, compare Watch scopes, inspect and export changes, review uninstall leftovers, and back up the local profile.
- [v0.3.0 release notes](releases/v0.3.0.md): full-profile backup before the schema v5 upgrade and how to roll back.
- [v0.2.0 release notes](releases/v0.2.0.md): legacy executable-adjacent data and the move to a per-user profile.
- [Command line](CLI.md): read-only `exor` commands with JSON output for scripts and CI (in the portable ZIP after v0.3.0, or built from source).

## Build, verify, or contribute

- [Development setup](DEVELOPMENT.md) and [architecture](ARCHITECTURE.md).
- [Windows release runbook](WINDOWS-RELEASE.md): build a candidate, promote its exact bytes, or verify a published release without launch. The optional launch observer and native checks require Windows 10/11 x64 clients.
- [Verification record](../AUDIT.md) and [desktop remediation details](AUDIT-REMEDIATION.md): which checks passed and which are still open.
- [Client evidence template](WINDOWS-RELEASE-EVIDENCE.md) and [screenshot checklist](SCREENSHOTS.md): how to record observations from a disposable test VM. Static package, browser-mock, and Windows Server results never fill a client row.
- [Roadmap](../ROADMAP.md) and [changelog](../CHANGELOG.md).

## Optional lab only

- [Live-source evaluation](LIVE-COLLECTION.md) and [registry-driver development](../driver/README.md). Neither live source powers desktop Watch or is required to use the released app.
