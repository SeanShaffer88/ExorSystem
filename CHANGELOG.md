# Changelog

## Unreleased
- Engine (`exor-engine`): tokio parallel walker, Merkle path-trie differential diff, BLAKE3 snapshots (incremental), noise filter, mock USN/registry event sources, JSONLogic policy simulator + templates, manifest quarantine (restore/purge), installer family fingerprinting, session monitor with confidence ladder (0.3/0.7/1.0), risk batching, portable plan, plain-language reports.
- Storage layer (`exor-storage`): blueprint SQLite schema (WAL), transactional migrations with pre-upgrade backup, node/edge/session/policy/ledger/quarantine handlers, DAG cycle rejection, provenance, shared-component impact, orphan review.
