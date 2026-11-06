# Folderloom: deliberate folder consolidation

Status: Synthesized from the accepted design interview and subsequent Czkawka evaluation. The user confirmed testing through the shared Rust job interface, with real temporary folders, SQLite, and process restarts. This spec is local; it has not been published to an issue tracker or labeled ready-for-agent.

## Problem Statement

Overlapping backups contain incomplete copies of the same collections. No single copy can be assumed complete because it is newer, larger, or contains more files. Unique files and differing versions may be distributed across several locations, while identical contents may intentionally appear in several meaningful subfolders.

The user needs one coherent destination without losing meaningful file placements, folder structure, differing contents, or the ability to trace a file back to its sources. A duplicate-file report alone cannot establish that every selected source contribution has reached the correct destination. Long scans and network interruptions make a saved inventory, trustworthy resume, and explicit verification necessary.

## Solution

A local desktop application performs deliberate consolidation jobs over multiple source roots and a chosen primary destination. It inventories folders and files, progressively compares contents, suggests corresponding folders, and presents an editable merge plan before copying anything.

The primary guides destination structure without making its file versions authoritative. The user can combine corresponding folders, choose custom destination names, or preserve a folder separately with its internal structure intact. Approved additions are copied and verified. Source cleanup is a separate reviewed operation, with recoverable cleanup as the default and permanent removal as an explicit alternative.

Every meaningful decision and completed operation is durable. Incomplete scans, unsupported preservation details, excluded entries, and failed operations stay visible. The first platform is macOS with local and OS-connected NAS folders; Windows follows, or ships early if actual build and filesystem validation are feasible without substantially delaying macOS.

### Delivery sequence

1. Workspace and read-only persistent inventory, exercised through a small CLI.
2. Staged hashing, folder matching suggestions, and saved merge plans.
3. Desktop review, persisted decisions, and job progress.
4. Approved copying and verification.
5. Recoverable cleanup, restore, and separately gated permanent cleanup.
6. Bounded real NAS validation and Windows assessment.

The first coding milestone is stage 1 only. Later capabilities in this spec are product requirements, not permission to add copying or deletion to that milestone.

## User Stories

1. As a user, I want deliberate jobs rather than continuous synchronization, so changes happen when I choose.
2. As a user, I want to select four or more source roots, so a job can cover all relevant backup locations.
3. As a user, I want to select several copies of one folder directly, so I can run a small focused consolidation.
4. As a user, I want to choose an existing or empty primary destination, so I can extend an organized collection or create a new one.
5. As a user, I want to choose a structure reference for an empty primary, so the proposed organization has a clear starting point.
6. As a user, I want every selected file and folder inventoried, so unique files do not disappear from consideration merely because they have no duplicates.
7. As a user, I want discovered entries to appear before hashing finishes, so I can see progress during long scans.
8. As a user, I want to pause, close, reopen, and resume a job, so interruptions do not discard completed work or falsely complete unfinished work.
9. As a user, I want unreadable files and incompletely scanned folders identified, so missing evidence is not mistaken for empty or fully processed content.
10. As a user, I want an Include or Ignore choice for empty folders, defaulting to Include, so structural preservation is intentional.
11. As a user, I want hidden files included by default, so hidden status does not silently exclude selected content.
12. As a user, I want suggested folder matches to show full paths and supporting evidence, so I can judge ambiguous names such as Desktop or Medical.
13. As a user, I want discovered candidates to begin unselected and support reviewed group selection, so I control which folders enter the merge.
14. As a user, I want matching to consider parent context and file contents, so identical folder names or common documents do not merge unrelated collections.
15. As a user, I want to confirm an exact matched folder, so its surrounding backup wrappers can be omitted without losing its meaningful descendants.
16. As a user, I want the primary structure to guide destination suggestions, so incoming folders fit my chosen organization.
17. As a user, I want unmatched source folders proposed as new folders, so their contents and internal structure remain available in the primary.
18. As a user, I want ambiguous mappings marked Needs mapping, so the program does not silently choose among plausible destinations.
19. As a user, I want to accept a mapping, choose another destination, or keep a folder separately, so suggestions remain under my control.
20. As a user, I want to enter an alternative destination folder name, so I can resolve conflicts using meaningful names.
21. As a user, I want identical contents at the same intended destination path to avoid redundant copying, so consolidation does not create unnecessary versions.
22. As a user, I want identical contents at different selected destination paths to retain each placement, so collections such as document packages remain complete.
23. As a user, I want differing file contents preserved with readable conflict names, so choosing a primary does not destroy another version.
24. As a user, I want conflict names previewed and retained across resume, so an interrupted job does not produce different or repeated suffixes.
25. As a user, I want to inspect paths, sizes, dates, and comparison status and open files in installed applications, so I can make decisions without a built-in document comparer.
26. As a user, I want to preserve a snapshot separately, so its internal names and structure remain recognizable instead of mixing with another snapshot.
27. As a user, I want to keep a dependent collection together, so resolving an outer-folder conflict does not rename files that refer to each other.
28. As a user, I want a saved merge plan before copying, so I can review exactly which source contributes to each destination.
29. As a user, I want proposed additions and verified additions distinguished, so a plan is not mistaken for completed work.
30. As a user, I want copied contents verified at their intended placements, so a matching file somewhere else does not hide a missing destination file.
31. As a user, I want source and destination changes detected before affected operations, so stale plans cannot overwrite or remove newly changed files.
32. As a user, I want independent work to continue when another item needs review, so one problem does not unnecessarily block the whole job.
33. As a user, I want original modification dates and supported metadata preserved, so consolidation retains useful file context.
34. As a user, I want preservation exceptions shown and held out of cleanup until reviewed, so an incomplete result is not called fully preserved.
35. As a user, I want links and their targets identified, so unresolved or broken references do not disappear into an apparently successful merge.
36. As a user, I want archives and backup containers preserved as whole files and recognized containers flagged for restoration, so preservation is not confused with examining their underlying documents.
37. As a user, I want to keep sources or separately review cleanup after verification, so copying does not imply deletion.
38. As a user, I want recoverable cleanup to use a visible default recovery folder organized by job, so removed originals can be found and restored.
39. As a user, I want to change the recovery location and review restore conflicts, so recovery works with my storage layout without overwriting newer files.
40. As a user, I want permanent removal to require an explicit choice and recovery never to purge automatically, so a recoverable operation does not silently become irreversible.
41. As a user, I want excluded, unreadable, changed, or unresolved source items retained, so cleanup cannot remove a folder that still contains unaccounted-for material.
42. As a user, I want primary and recovery folders excluded from incoming discovery and source cleanup, so the application does not recursively process or remove its own results.
43. As a user, I want job records stored locally and an exportable source history, so NAS disconnection does not hide progress and the final collection can carry its provenance.
44. As a user, I want processing to remain local, so file contents do not need to be uploaded to a hosted service.
45. As a user, I want a standalone desktop window on my Mac and eventual Windows support, so everyday use does not require browser tabs or terminal commands.

## Implementation Decisions

### Architecture and reuse

- Use one repository with a Cargo workspace, a reusable Rust core, a development CLI, and a Tauri desktop host with a React/TypeScript interface.
- The Rust core owns inventory, comparison, folder mappings, plans, execution, verification, recovery, and source history. CLI and GUI use the same public job interface. Desktop-framework types must not leak into the core's public interface.
- Keep the initial public interface small: create/configure a job, inspect durable progress and inventory, pause, and resume. Add planning and execution operations only in the corresponding later milestones.
- Use local SQLite for durable state outside source roots and the source repository. Scanning and comparison work locally against paths exposed by the operating system; direct SMB protocol support is deferred.
- Build Folderloom's workflow rather than forking the whole Czkawka application. Reuse established libraries and adapt appropriately licensed techniques when useful. Direct inclusion of a broad upstream core requires evidence that its interface, dependencies, and persistence fit the requirements.
- BLAKE3 is the proposed full-content hashing library. Sample sizes, small-file thresholds, and concurrency settings require measurements; no fastest configuration has been established.

### Durable state and comparison

- Model consolidation jobs, source roots, the primary, discovered entries, directory-enumeration progress, observations, comparison evidence, mapping decisions, plan revisions, action results, preservation exceptions, and recovery records as distinct persisted concepts. Exact table and field names remain implementation choices.
- Record file placements independently of content fingerprints. A unique-sized file may leave the duplicate-candidate pipeline but must remain in the inventory and merge coverage.
- Distinguish discovered, pending examination, fully examined, failed, and stale work. Keep directory-enumeration completion separate from file-hashing completion and whole-job completion. The absence of a persisted child is not proof that enumeration finished.
- Preserve path identity without lossy conversion; display formatting must not merge distinct source names. Source identifiers and observations must support detecting changed roots and remounted shares rather than trusting a pathname alone.
- Use size and optional partial hashes to screen duplicate candidates. Matching samples do not establish full-content equality. Store sample format and hash algorithm with evidence, and never confuse a partial result with a full hash.
- Commit completed work in bounded durable checkpoints. An abrupt exit may require repeating an uncommitted batch, but cannot cause uncommitted or interrupted work to appear completed. Resume must avoid duplicate inventory entries and revalidate stale observations.
- Cached size and dates are useful change signals, not sufficient authority for consequential cleanup. Fresh content, identity, and destination-coverage checks are required for the affected operation.

### Mapping and review

- Suggestions use names, context, structure, and content evidence. Automatically discovered candidates require selection. Generic names and isolated shared documents do not establish collection identity.
- The selected matched folder defines the source-to-destination mapping. Omit source ancestors above it from the resulting path while retaining the complete original path in source history.
- Follow the primary's preferred structure where a mapping is confirmed. Propose new folders when no plausible counterpart exists and hold ambiguous mappings for review.
- Resolve differing-content file collisions with readable source labels and collision-checked numbering. Persist proposed names. Check the destination's actual naming rules, including case differences, invalid names, and file-versus-directory conflicts.
- Permit custom destination folder names and update all affected proposed paths before approval. Editing a plan produces a revised set of operations; approval of an earlier version does not approve later changes.
- Separate preservation resolves collisions at the outer folder and retains internal names and paths. Do not merge later additions into a preserved snapshot. Unsupported fidelity is an exception, not silently reported success.

### Execution, verification, and cleanup

- Source inspection does not create application files or modify source entries. Keep databases and logs outside inspected roots. Filesystem-managed access-time changes are not an application write guarantee; assess read-only behavior through content, names, and relevant preserved metadata.
- Revalidate affected sources, destinations, and plan decisions before operations. Never overwrite an unexpected destination entry or delete a changed source using stale evidence. Serialize conflicting application writes.
- Stage copies, verify contents and required preservation properties, and publish without clobbering unexpected destination files. A partial copy or failed preservation step cannot be marked fully verified.
- Include hidden files by default. Ask about empty folders once per job, defaulting to Include. Record exclusions, and do not describe a result omitting empty folders as an exact structural copy.
- Preserve links where supported, expose targets and risks, and do not automatically traverse outside selected sources. Do not silently rewrite links or replace them with target contents. Unsupported relationships hold affected cleanup for review.
- Treat containers as whole files in the first version. Identify recognized backup containers when possible and distinguish copying the container from restoring or comparing its contents.
- Exclude primary and recovery areas from incoming traversal, account for overlapping roots, and prevent self-copying. Primary files are not source-cleanup candidates.
- Cleanup is separately reviewed and covers only eligible, individually verified source entries. A destination content match is insufficient if the required placement or preservation properties are missing.
- Recovery defaults to a Folderloom recovery directory on the source drive or share, partitioned by job identifier. It is configurable, retains original-path records, supports conflict-aware restore, and never purges automatically. If unavailable, request another location rather than silently deleting permanently.
- Permanent cleanup is a separate explicit choice. Never remove a source folder containing excluded, unreadable, changed, or unresolved material. Ignoring an empty folder is not authorization to delete it.
- Record each operation durably so resume cannot repeat completed destructive work or confuse incomplete transfers with verified results. An exception review must create an explicit decision; merely viewing a warning does not authorize cleanup.

### First milestone acceptance criteria

1. A development CLI invokes the reusable core to inventory a small explicitly selected folder tree.
2. Inventory includes ordinary files, hidden entries, empty folders according to the selected policy, and observable failures. Unique files remain present without requiring duplicate matches or hashing.
3. A new process can reopen the saved job and display its inventory and progress.
4. Cooperative pause and abrupt process termination can be followed by a resume that produces the same complete inventory for an unchanged fixture, without duplicate entries or falsely completed directories.
5. Unreadable or interrupted enumeration stays explicit. The job distinguishes finishing all attempted work from completely examining every selected entry.
6. Changed or unavailable roots do not become empty successful scans. Stale observations cannot silently be presented as current.
7. Source content, names, and relevant metadata are unchanged by the scanner; no database, log, or checkpoint is written into source roots.
8. No copy, merge, recovery move, or deletion action is exposed by this milestone.

## Testing Decisions

### Confirmed seam

The primary test seam is the Rust core's public job interface, also used by the CLI and desktop app. Tests create real temporary filesystem trees and local SQLite stores, invoke externally meaningful job operations, close and reopen state, and inspect observable inventory, plans, file placements, and history. Process-level tests exercise termination and restart through the thin development runner.

Test externally observable behavior and guarantees, not private helper names, SQL statement text, thread scheduling, or data-structure layouts. Add internal seams only where a concrete failure cannot be exercised reliably through the public interface. There is no existing test-suite prior art in this repository: the current application is the generated Hello World program. Upstream Czkawka tests may inform cases but do not validate Folderloom.

### Acceptance matrix

| Milestone | Scenario | Observable result |
| --- | --- | --- |
| Inventory | Nested, hidden, empty, and unique entries | Expected inventory and explicit empty-folder exclusions; no source writes |
| Inventory | Pause or terminate during directory enumeration | Reopen and resume; no missed children or false completion |
| Inventory | Unreadable entry or unavailable root | Failure remains visible; no claim of empty or fully covered input |
| Inventory | Reopen after source edits or root replacement | Stale observations identified; affected work re-examined |
| Comparison | Same size and matching sampled edges, different middle | Full comparison reports differing contents |
| Comparison | Unique size or sample | File remains inventoried and eligible as an addition |
| Comparison | Pause or terminate during hashing | Incomplete evidence is not usable as a full hash; completed checkpoints survive |
| Mapping | Same generic names in unrelated collections | No automatic consolidation based on name alone |
| Mapping | Confirmed folder at a deeper backup nesting level | Omitted ancestors remain in source history; descendants map correctly |
| Planning | Identical contents at two meaningful destination paths | Both placements are included or already verified at their exact paths |
| Planning | Ambiguous mapping, custom name, or file/directory collision | Explicit review state and collision-free saved plan |
| Planning | Reopen after edits and approvals | Mapping decisions, chosen names, and plan revision remain consistent |
| Preservation | Snapshot or dependent package with outer collision | Outer name changes; internal paths remain intact |
| Copy | Destination appears or source changes after approval | No unexpected overwrite; affected operation requires replanning |
| Copy | Interruption, insufficient space, or permission failure | No incomplete result marked verified; resume does not duplicate successful work |
| Preservation | Unsupported metadata or broken link | Explicit exception; affected cleanup is held |
| Cleanup | Excluded, changed, unreadable, or unresolved source entries | Those entries and nonempty containing folders remain |
| Cleanup | Recovery unavailable or restore target occupied | No permanent fallback and no unreviewed overwrite |
| Cleanup | Termination before or after a recorded operation | Reopen determines actual state without repeating destructive work |
| Scope | Primary or recovery nested beneath a source root | Neither becomes incoming material or a source-cleanup target |
| Container | Recognized proprietary backup file | Container preservation is distinguished from document restoration |

Add desktop tests once the UI exists to prove source selection, review edits, pause/resume, and approval display persist after reopening and invoke the same core rules. Full engine behavior must not depend on UI tests. Network-share tests and Windows checks are separate evidence from local fixture tests; do not claim platform support from compilation alone.

## Out of Scope

- Continuous synchronization and automatic propagation of deletions.
- Hosted processing, cloud accounts, or upload of file contents for comparison.
- A custom network-file protocol client in the first version.
- Automatic restoration of proprietary backup containers or comparison of their underlying documents before restoration.
- Built-in document previews, semantic document comparison, image similarity, and audio fingerprinting in the first version.
- Automatic collapse of distinct meaningful file placements solely because their contents match.
- Migration of unfinished jobs between computers.
- Automatic recovery-folder expiry or purge.
- A whole-application Czkawka fork.
- Copying, cleanup, or desktop delivery in the initial inventory milestone.

## Further Notes

- The existing design, glossary, architecture decision, and Czkawka source evaluation remain supporting records. This spec synthesizes their agreed behavior into implementation and testing requirements.
- Czkawka was reviewed at commit 7b2383f88baf73aac99c99992fe54321b63b7f6a. Its staged filtering is useful prior art; its duplicate results and hash cache are not Folderloom's complete inventory or operation journal.
- Exact hash tuning, cache-validity mechanics, platform metadata coverage, and Windows release timing require implementation evidence. No NAS performance benchmark has been run for this design.
- Real client data, inventories, and source paths must remain outside committed fixtures. Use synthetic data for development and tests; validate real NAS behavior only on explicitly selected bounded data.
- No Git remote, project issue tracker, or configured triage-label vocabulary was found. The to-spec workflow expects project setup and eventual publication with ready-for-agent; neither publication nor that label has occurred.
