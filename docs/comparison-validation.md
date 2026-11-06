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

## Scope and limits

Saved observations do not assert that sources remain unchanged after the last inspection. Sample matches cannot prove equality. Full hashes establish saved content evidence for an observed generation and placement; they are not destination coverage or cleanup authorization. Completed evidence and incomplete read checkpoints are distinct.

Local validation uses macOS and the installed SDK override `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk`. Linux CI is separate evidence. Windows, real NAS behavior, adversarial filesystem races, folder mappings, merge plans, copying, and cleanup are outside this delivery.
