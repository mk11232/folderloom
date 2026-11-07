# Folderloom design

Status: Product design and architecture baseline accepted at interview question 34. Product decisions are recorded in [requirements.md](requirements.md); terms are defined in [../GLOSSARY.md](../GLOSSARY.md). The user subsequently chose Folderloom's own workflow with established libraries and ideas from Czkawka rather than a whole-application fork; no source-file operation is authorized by this document alone.

## Product

A local desktop application for deliberate consolidation of overlapping folder collections into a chosen primary destination. A job can include multiple source roots, or several directly selected copies of an individual folder. Preserve meaningful file placements, differing contents, and source history.

macOS and OS-connected NAS folders are the initial target. Choose portable foundations for Windows; include Windows early only if actual build and filesystem validation are feasible without substantially delaying the Mac version.

## Desktop navigation

The implemented desktop opens to a saved Jobs table. New job guides Sources → Primary destination → Scan and compare → Merge plan → Results. Setup selections can be removed without deleting files. Started jobs retain their original source set; Use as new job creates a separate draft for changes. See [the job workflow](jobs-workflow.md). The broader product workflow below still includes deferred features.

## Workflow

1. Configure sources, primary destination, source labels, and whether to include empty folders (default Include). For an empty primary, select a structure reference.
2. Discover folders and files and persist progress. Display inventory before full-content comparison finishes. Pause and resume without confusing unfinished work with completed work.
3. Suggest groups and destination mappings using context, names, structure, and content evidence. Display exact source paths and proposed destinations. Discovered candidates require selection; review can operate on groups.
4. Resolve additions, uncertain mappings, and conflicts in a saved plan. Allow custom destination folder names. Preserve different contents using readable source-label suffixes and collision-checked numbering.
5. Copy approved additions and verify their contents and required preservation properties. Preserve existing primary files; replan items changed since approval.
6. Report verified placements, exclusions, preservation exceptions, and unresolved work. Offer an export of the plan and source history.
7. Separately review source cleanup: keep originals, recoverable cleanup, or explicit permanent removal. Recovery defaults to Folderloom/Recovery/<job-id>/ on the source drive or share, is configurable, and is never automatically purged. Restore operations include conflict review.

Identical contents at different selected destination paths still populate each path. An identical file elsewhere in the primary is not coverage of the required placement.

## Separate preservation

Offer Preserve separately / Keep this folder together for snapshots and collections whose internal paths matter. Resolve conflicts at the outer destination folder and keep its internal paths intact. Snapshot preservation and source cleanup are independent choices. Ignored empty folders and unsupported metadata must remain visible as preservation exceptions.

## Accepted architecture baseline

- Rust core library owns inventory, comparison, mapping, plans, copying, verification, recovery, and history. Keep a small interface for job operations and progress; hide filesystem and persistence details inside the core.
- Local SQLite stores durable inventory and job state. Store real databases outside the source repository and outside scanned roots.
- Tauri desktop shell calls the Rust core. React and TypeScript implement the folder trees, tables, plan review, and progress views. Bundle the interface locally; it opens in the app's own window and requires no hosted website or cloud processing.
- A small CLI calls the same core for early scanner development and diagnostics. Everyday use belongs in the desktop GUI.
- The GUI cannot bypass core validation or perform independent copying and deletion. An approved plan is revalidated at execution; UI state alone is not authority to act on paths.

Tauri supplies a desktop window with an embedded system webview and a Rust host. Its documented architecture supports macOS and Windows. The user accepted this desktop approach at question 34; platform testing remains outstanding.

Sources checked: [Tauri overview](https://v2.tauri.app/start/), [Tauri architecture](https://v2.tauri.app/concept/architecture/).

The user identified Czkawka as the upstream hashing example and accepted retaining Folderloom's own application and durable inventory, using established libraries and selectively learning from or adapting upstream techniques. The [source review](czkawka-evaluation.md) records the evidence. No fork or dependency integration has been performed. Richer image, audio, and document analysis remain possible later extensions.

## Repository layout

```text
folderloom/
  Cargo.toml                      Rust workspace
  Cargo.lock
  crates/
    folderloom-core/
      src/lib.rs
      src/inventory/
      src/compare/
      src/plan/
      src/execute/
      tests/
    folderloom-cli/
      src/main.rs
  apps/
    desktop/
      src/                        React and TypeScript interface
      src-tauri/                  Rust desktop host
  docs/
  GLOSSARY.md
```

Introduce modules only as their behavior is implemented. Keep core independent of desktop-framework types so CLI, GUI, and tests use the same rules.

## Engineering requirements to prove

- Partial fingerprints are candidate filters, never completed full-content evidence. Record full hashes and their algorithm; invalidate stale evidence. Metadata alone is not proof of unchanged contents before consequential cleanup.
- Distinguish directory discovery completion, individual file examination, and whole-job completion. Unreadable directories or interrupted enumeration cannot imply an empty or fully covered tree.
- Before copying or cleanup, revalidate affected source identity, content, destination placement, and relevant filesystem conditions. A reconnected path must not silently identify a different share or directory.
- Publish verified copies without overwriting unexpected destination entries. Handle collisions under the destination filesystem's actual naming rules, including case differences and file-versus-directory conflicts.
- Persist action state so interruption cannot turn a partial copy into a completed item or cause repeated cleanup. Serialize conflicting write operations and stop affected items on external changes.
- Exclude the primary and recovery areas from incoming discovery; detect overlapping roots and self-copying. Never clean up primary files.
- Preserve links where supported, expose targets and unsupported relationships, and hold affected cleanup when resolution is uncertain. Preserve recognized packages together; do not claim universal internal-reference detection.
- Validate free space, permissions, unsupported metadata, and recovery-path suitability before the affected operations. Report limitations and never silently fall back to destructive behavior.

## Implementation sequence

1. Establish the workspace and demonstrate the CLI calling the core with synthetic data.
2. Build the read-only persistent inventory: select a small root, save results, reopen, interrupt and resume, record failures, and demonstrate no source modifications.
3. Add staged hashing, folder suggestions, and durable review plans; no source mutation.
4. Add the desktop review workflow with persisted edits and pause/resume.
5. Implement copy and verification against synthetic fixtures, including conflicts, stale plans, interruptions, and preservation exceptions.
6. Implement recoverable cleanup and restore, then separately gated permanent cleanup, with failure-path validation.
7. Validate the actual macOS/NAS workflow on explicitly selected small data and assess Windows using real build and filesystem checks.

## Deferred scope

Continuous synchronization, direct SMB protocol support, automatic restoration of proprietary backup containers, document-internal comparison, built-in previews, and migration of unfinished jobs between computers. Inventory and preserve containers as files while clearly distinguishing that from restoring their underlying documents.
