# Staged comparison validation

Issue [#4](https://github.com/mk11232/folderloom/issues/4) delivers read-only comparison of saved inventory placements. Tests use the agreed public `Job` interface and development CLI with real temporary files, SQLite stores, and subprocess restarts. Fixtures are synthetic. No test authorizes copying or deletion.

## Red and green evidence

The first CLI slice creates two identical contents at different paths and a unique file, completes inventory, compares, then opens saved evidence in another process. Before implementation, the focused test failed with `folderloom: unknown option "--byte-budget"`. After the CLI adapter and core checkpoint `2bfe5ee`, that same focused test passed. The adapter uses the same core comparison operations as other callers; it does not implement hashing itself.

A second CLI red/green slice required saved evidence to identify the source root behind each root ID. The focused test first failed because only relative paths were printed, then passed after adding the saved root ID/path mapping.

Additional process tests validate the finished public behavior:

- A small identical pair completes hashing while a larger pair reaches a durable partial full-read checkpoint. A concurrent writer is rejected. The runner is killed after the committed checkpoint; a separate read-only process and `Job::open` both expose incomplete full evidence. Previously completed digests survive. A new writer cooperatively pauses, then another resumes to complete hashes without changing the inventory generation. The interrupted record has a sample but no full hash until completion.
- A source file changes after complete comparison. Read-only saved status stays a saved observation. Running comparison returns exit 2 with explicit failed evidence and no full hash for the changed placement. Inventory resume invalidates prior comparison; status reports `Invalidated` and old records show `stale=true`. A comparison in the new generation completes and retains both placements.
- The existing inventory subprocess test still kills a runner during incomplete directory enumeration and resumes through a cooperative pause.

These added behavior checks passed against the first implementation. The interruption test's initial harness run closed the pause process's stdout before its final checkpoint and failed with a broken pipe; retaining the reader fixed the harness. This was not a product regression or a claimed implementation red.

## Core coverage and integrated validation

The core's first tracer test failed on the missing public comparison API, then passed after implementation at `2bfe5ee`. It checks independent placements, unique-file retention, and the known canonical BLAKE3 digest of `abc`. Nine further public-interface regression tests passed when introduced; they are coverage, not additional observed red/green cycles.

The ten core comparison tests cover canonical full hashes for ordinary and empty content; matching edge samples with different middle contents; byte-bounded reads; unique samples; pause/reopen and incomplete-read restart; completed evidence reuse; changed, missing, unreadable, and link-replaced sources; root and ancestor replacement during reads; generation invalidation; legacy schema migration and required rescan; single-writer/read-only behavior; and preservation of source contents, names, mtimes, hidden entries, symlinks, and empty-directory exclusions. Invalid UTF-8 placement coverage runs on Linux because macOS APFS rejects those fixture names.

At integrated branch checkpoint `183ddf6`, local macOS validation passed `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`: 26 tests total (4 CLI process, 10 comparison, 12 inventory). This is local evidence; CI results are reported separately.

## Scope and limits

Saved observations do not assert that sources remain unchanged after the last inspection. Sample matches cannot prove equality. Full hashes establish saved content evidence for an observed generation and placement; they are not destination coverage or cleanup authorization. Completed evidence and incomplete read checkpoints are distinct.

Local validation uses macOS and the installed SDK override `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk`. Linux CI is separate evidence. Windows, real NAS behavior, adversarial filesystem races, folder mappings, merge plans, copying, and cleanup are outside this delivery.

The byte budget bounds source-content reads per checkpoint. It does not bound database iteration, metadata I/O, or waiting on an unavailable share. This serial implementation has no fastest-hash or NAS performance claim. Schema v1-to-v2 writer upgrades are one-way for the job database; read-only v1 observers do not migrate it, and old inventory observations need a fresh scan to supply identity evidence.
