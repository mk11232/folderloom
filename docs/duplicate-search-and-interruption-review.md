# Duplicate search, interruption, and scale review

Date: 2026-11-07. Status: proposed implementation sequence, not delivered functionality.
Reviewed Folderloom HEAD: `d67ca4a`. No NAS benchmark or upstream execution was performed.

## Review provenance

Kimi K3 reviewed a bounded text packet describing the current repository and proposed roadmap. The local CLI request recorded model `k3`, alias `kimi-code/k3`, and `thinkingEffort: max`; session `session_108beadd-7a03-4bb9-bc6d-fed5364dc6e3`. It did not independently inspect source or run tests. Its suggestions were critically reviewed below. Claude Fable 5.1 subsequently completed a bounded text-only challenge through the agent bridge (job `2468ae69-bbd4-4b3c-a941-d67476c007da`; reported model `claude-fable-5-1`). It reviewed the supplied evidence and proposals, not the source code or runtime.

## Evidence and inspiration

The [README](../README.md) explicitly points to Czkawka and the [existing pinned evaluation](czkawka-evaluation.md), not a broad catalog of inspiration projects. The [current upstream README](https://github.com/qarmin/czkawka) describes Krokiet as its newer GUI, Czkawka as its legacy GTK GUI, and separate tools for duplicates, empty folders, large files, and media. These are related frontends around a shared core, not independent evidence for multiple competing architectures.

Borrow the separation of task intent and the staged duplicate pipeline, not the entire cleanup toolbox. The pinned evaluation records size screening, samples, full hashes, and cache hints, with no demonstrated abrupt-kill per-file durability. Folderloom already has staged comparison and BLAKE3; retain those capabilities rather than rebuilding a scanner or removing sampling to simplify the roadmap.

Current local limits:

- [Desktop host](../apps/desktop/src-tauri/src/lib.rs): one worker, cached snapshots, pause request, and no dedicated close/quit event handling in the run entry point.
- [Consolidation workflow](consolidation-workflow.md): planning commits an entire plan atomically without mid-plan pause; copying pauses between files. A large file can delay a cooperative pause.
- [Comparison validation](comparison-validation.md): incomplete full reads restart; completed evidence persists. These historical tests support specific recovery behavior, not unlimited crash guarantees.
- [README](../README.md): full inventory is loaded; large-library and NAS responsiveness remain unvalidated.

## Revised implementation order

### 1. Responsive stopping and honest recovery

Intercept native close and application quit during active work. Offer **Pause and quit** and **Keep working**. Once requested, show **Saving progress…** and exit only after the worker acknowledges a durable checkpoint. Do not label a requested pause as completed. Let the user cancel the pending quit while the app is still responsive.

Add cancellation checks within long hashing/copying loops and during planning. Preserve the last complete plan; do not expose a partial plan for approval. An interrupted copy must remain in its owned staging area, never appear as a verified completed destination. Resume can restart the unfinished file initially; resumable partial copies are a later optimization.

Chunk checks cannot interrupt every blocked filesystem call. A disconnected NAS can delay acknowledgement; show that state honestly. OS force quit remains distinct from cooperative pause and uses the tested recovery path. Do not promise that every crash loses only one file of work or that an interrupted process can first mark itself interrupted.

Cancelling setup continues to discard the draft. Stopping an already-started job preserves its history. Deleting job history remains a separate action from deleting source files.

### 2. Small read-only duplicate workflow, with bounded data delivery

Offer **Find duplicate files** and **Consolidate folders** at setup. Duplicate search requires sources but no destination or merge plan. Reuse the durable inventory and comparison engine, retaining unique entries, empty directories, errors, and every placement for later reconstruction.

Return paged groups and placements from SQLite; avoid serializing the entire inventory before paginating in the browser. Use bounded UI rendering. This minimum scale work belongs in the first duplicate workflow, before the broader performance milestone.

Show full-hash matching groups, all locations, scope, incomplete roots, exceptions, and evidence freshness. Samples are candidates only. No automatic keeper, newest/largest recommendation, source deletion, or move action. Equal bytes at different locations may be intentional.

Report **logical duplicate bytes**, with hardlink aliases handled explicitly. Do not label this guaranteed recoverable disk space: hardlinks, clones, compression, sparse files, snapshots, and filesystem allocation can change the actual amount reclaimed. Empty files are valid equal-content results with zero content bytes, not inherently disposable.

### 3. Measured large-library and NAS readiness

Extend paged querying to the existing inventory/tree/search views. Bound memory, database transactions, and simultaneous readers. Keep one active filesystem job initially; a visible queue can follow if needed. Do not introduce parallel jobs merely because multiple saved jobs exist.

Benchmark many small files and huge files separately, including large duplicate groups, mostly unique files, and slow/disconnected storage. Record wall time, bytes read, peak memory, UI latency, and pause acknowledgement latency. Choose measurable release thresholds from a documented baseline; no speed or NAS scale claims before those results exist.

### 4. Reviewed cleanup later

Keep source cleanup separate from discovery and copying. Require fresh identity/content checks, explicit placement review, recovery design, and an audit record before any destructive workflow. Job database deletion does not implement file cleanup.

## Accepted and rejected review points

Accepted from Kimi: responsive interruption precedes broad claims; duplicate search can precede the full scale milestone; paged results are a prerequisite; reuse the existing inventory; expose partial scans; distinguish equality from removability; defer parallel jobs and cleanup.

Qualified or rejected: a four-action quit dialog; a blanket claim that current copy integrity is unprotected (the existing workflow has staging and verification); force-kill promises of exactly one-file rework; treating duplicate-only scans as inherently small; claiming recoverable bytes from hashes and inode counts alone; treating stat equality as proof of unchanged content; discarding existing sampling; requiring hashes for every unique file just to preserve its inventory entry. These exceed the supplied evidence or add unnecessary scope.

## Claude Fable challenge and disposition

Accept: split the first milestone into a small native quit lifecycle change and a subsequent responsiveness change. First establish explicit running, quit-requested, checkpoint-acknowledged, and exiting states using the existing safe boundaries. Repeated Cmd-Q must not duplicate dialogs or force exit. Cancelling a pending quit must invalidate that exit request so a late acknowledgement cannot close the app. Account for both window-close and application-quit events. Show when work is still waiting; do not claim responsive huge-file stopping until within-file cancellation is delivered and measured.

Keep within-file cancellation in the next bounded milestone rather than defer it indefinitely: large files are a user requirement. Use the existing byte-bounded comparison behavior where available; do not regress it to waiting for a whole hash simply to share the copy lifecycle.

Accept as validation requirements, not diagnosed bugs: test interruption between output publication and its durable completion record, and verify idempotent recovery using existing plan/journal ownership and fresh destination checks. First inspect the current implementation; the supplied packet does not establish that reconciliation or same-volume staging is absent. A matching size/hash alone does not prove a file was created by this job, and recovery must never delete or overwrite unrelated output.

Accept: paged results need stable ordering and an explicit inventory/comparison revision. Pin a completed result revision or refresh explicitly when it changes; cursor paging alone does not freeze changing groups. Counts and rows must refer to the same revision. Test groups that split or change during comparison without presenting samples as confirmed duplicates.

Add coverage for system termination during blocked I/O, case/Unicode path aliases, overlapping roots, and filesystem-dependent identity evidence. Distinct roots can contain distinct files with the same name; case similarity alone cannot establish shared identity. Treat uncertain network-file identities as uncertain rather than claim hardlinks from unvalidated identifiers.

Reject blanket assertions in the text review: that NAS SQLite durability is inherently untestable; that all SMB rename behavior has one guarantee; that every destination on disk must have a Folderloom record; and that checking an atomic quit flag inside a SQL transaction by itself closes the lifecycle race. The job database already lives locally per the README. Define synchronization around worker acknowledgement and new-work admission, and scope reconciliation invariants to outputs owned by the job. Historical completion evidence does not promise that externally mutable files remain unchanged forever.

## Acceptance checks before release

1. Native close and Cmd-Q during scan, comparison, planning, copy, and verification: keep working continues; pause-and-quit waits for acknowledgement; reopening shows durable state.
2. Pause in a huge file and a throttled filesystem: measure latency; incomplete staging never becomes verified output; cancelling a pending quit remains possible. Separately exercise blocked/disconnected storage.
3. Kill at multiple checkpoint/publication boundaries and reopen: database is recoverable, incomplete work is never reported complete, prior valid results are retained/revalidated as appropriate, and sources are unchanged. Measure rework rather than assume its maximum.
4. Equal size and equal head/tail with a different middle: no full-hash match. Changed, replaced, unreadable, missing, and reconnected sources: stale or incomplete evidence is visible and not silently trusted. Metadata alone cannot detect every content change.
5. Hardlinks, empty files, overlapping roots, and intentional matching placements: locations remain intelligible and logical-byte totals do not claim disk reclaimability.
6. Partial scan or disconnected root: completed scope and gaps remain explicit; an incomplete enumeration cannot masquerade as a completed inventory or authorize removal.
7. Large synthetic inventory and duplicate group: database queries, IPC payloads, and rendered rows remain bounded; measure resource use and responsiveness before selecting a supported scale.
8. Repeated quit requests, cancellation racing a late acknowledgement, last-window close, and shutdown interruption: no accidental exit, duplicate prompt, or orphaned active worker.
9. Publication-before-record failure injection: owned output is reconciled without overwriting unrelated files; unknown ownership is held for review.
10. Paging during scan/comparison changes: a pinned result revision has stable membership and ordering, consistent counts, and explicit refresh behavior.
11. Multiple saved jobs: only one worker runs; queued work, if introduced, starts only after the preceding worker releases ownership.

No product code changed and no new runtime validation was performed for this research note.
