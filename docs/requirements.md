# Folderloom requirements interview

Status: Shared product design and architecture baseline confirmed at question 34. Implementation details and upstream reuse remain under evaluation. Product approval does not authorize operations on real source files.

## Confirmed requirements

- Work happens through deliberate consolidation jobs, not continuous synchronization.
- A job supports multiple source roots or directly selected folders, including four or more locations; comparison is not restricted to two sides.
- Support manual selection and discovery of potentially related folders beneath selected roots. Suggested uncertain matches require confirmation before consolidation.
- The primary folder controls the destination and preferred folder structure, not which differing file contents automatically win.
- Add unique files, skip copying identical files at matching locations, and preserve differing files for review. Dates alone do not determine which version survives.
- Track proposed additions and completed, verified additions through persistent application status and source history. Preserve original filenames unless a naming conflict requires a distinct name.
- Propose additions in a merge plan before physically copying them. Copy and verify after review and approval; source removal is a later, separate step.
- Use Rust for the engine.
- Provide a standalone desktop GUI with its own window, folder pickers, and comparison views. Everyday use must not require browser tabs or terminal commands. Tauri with React/TypeScript and a Rust core is the accepted architecture baseline.
- Support both collection jobs (multiple roots mapped to a primary root) and individual jobs (multiple corresponding folders mapped to a chosen primary folder).
- A primary destination may already exist or be new. Show proposed destination paths, including folders absent from the primary, before copying.
- Present candidate source folders with full paths, matching evidence, expandable contents, proposed additions, and inclusion controls. Automatically discovered candidates start unselected; support selecting a reviewed group together.
- Generic names alone do not establish folder identity. Use parent context, content overlap, internal structure, and confirmed mappings as evidence for suggestions.
- Discover possible matches at different depths. Once an exact matched folder and destination mapping are confirmed, omit all source parent folders above it from the destination path. Preserve descendants' relative structure unless an alternative mapping is approved. Retain the full original source path in source history.
- For differing contents at a conflicting destination path, preserve the existing file and give the incoming file a readable source-label suffix, adding a number if needed. Preserve the extension, check for collisions, show the exact name in the plan, and retain it when resuming. UUIDs or timestamps are not required in visible filenames.
- Consolidate selected file placements into the primary, including identical contents at different mapped paths when those placements are being preserved. A content match elsewhere in the destination must not leave a selected placement available only in an old source. Any deliberate removal of duplicate placements requires an explicit choice.
- Cleanup is separately approved and limited to individually verified, approved source files. Excluded, unreadable, and unresolved files remain in place; do not remove their containing source folder. Explain outstanding contents in the interface.
- The user emphasized one final location with fidelity to meaningful folder structures, including structures found in backups. Default to a combined collection, with an explicit option to preserve a selected folder separately as a snapshot with its internal tree intact.
- For either consolidation or separate preservation, allow leaving the source unaffected or requesting removal after verified copying and separate cleanup approval. This is a product requirement, not authorization to remove current source files.
- Use the primary's existing folder structure to guide suggested mappings for differently named source subfolders. Show supporting evidence and allow the user to accept a mapping, choose another destination, or preserve a separate folder.
- If a source folder has no plausible counterpart, propose it as a new folder with its contents and internal structure intact. If several destinations are plausible, mark it Needs mapping rather than silently choosing one.
- For an empty primary destination, let the user select a source as the structure reference. Other sources contribute additions and conflicts; the reference does not confer authority on file versions.
- Allow the user to enter an alternative destination folder name when a conflict arises. Show the resulting paths in the plan before approval; this does not rename the original source folder.
- For preserved snapshots, resolve destination collisions at the outer snapshot folder. Preserve internal filenames and relative paths, do not merge later additions into that snapshot, and report preservation exceptions rather than silently changing the internal structure.
- Offer recoverable cleanup by default and an explicit permanent-removal option against the verified cleanup list. Explain that recovery retains storage usage until emptied. If recovery is unavailable, do not silently substitute permanent deletion.
- Persist progress and support resuming interrupted jobs. Recheck affected sources and destinations; changed items require an updated plan while independent items may continue. Never overwrite newly appeared destination files or remove changed source files based on stale verification.
- Persist a per-file inventory with examination progress and content hashes so pause/resume can distinguish completed, pending, failed, and stale work. Exact cache-validity rules remain to be specified.
- Display folders and files as discovered, with content comparisons filled in as hashing progresses. Clearly label unfinished comparisons and permit pause/resume throughout. Partial fingerprints can screen candidates but do not establish full-content equality or eligibility for duplicate skipping or cleanup.
- Ask once during job setup whether to include or ignore empty folders; default to Include. Record ignored empty folders as exclusions in the plan and do not call that result an exact structural copy. Ignoring an empty source folder does not itself authorize its deletion.
- Prioritize macOS with local and OS-connected NAS folders. Support Windows later, or in the first release if building and testing it does not substantially delay the Mac release. The user has a Windows PC as well; retain portability when selecting the desktop framework.
- In the first version, inventory and preserve archives and backup containers as whole files. Label recognized backup containers as requiring restoration to compare their underlying contents. Restoration is a separate later workflow; do not claim contained documents have been compared.
- Include hidden files by default and preserve modification dates. Preserve other metadata where supported, report omissions or preservation failures, and hold affected items out of cleanup until reviewed.
- If the primary is nested within a selected source root, exclude it from incoming material and examine it separately as the destination reference. Show this exclusion, prevent self-copying, and never include primary files in source cleanup.
- Preserve links as links where supported without automatically following them outside selected sources. Show targets, flag links that would break, and require resolution before affected source cleanup. Do not silently rewrite links or substitute target contents.
- Offer Keep this folder together using separate-preservation behavior: resolve collisions at the outer folder and preserve internal paths. Suggest it for recognized packages and permit marking any folder manually; do not promise to detect every internal document dependency.
- Recoverable cleanup uses a configurable recovery location, defaulting to Folderloom/Recovery/<job-id>/ on the source drive or share where writable. Group files by job, retain their original paths in recovery records, exclude recovery areas from scans, provide Restore with conflict review, and never purge automatically. If the default cannot be used, request another recovery location rather than silently deleting.
- Store the working inventory and job progress on the user's computer and perform scanning and comparison locally. Offer an export of the plan, source history, and verification results beside the consolidated collection. Moving unfinished jobs between computers is deferred.
- First-release file inspection shows paths, sizes, dates, and comparison status, with Open and Show in Finder/Explorer actions. Built-in previews and analysis of changes inside documents are deferred.
- Build Folderloom's own consolidation workflow using established libraries and ideas from Czkawka rather than forking its whole application. Image, audio, and document analysis are possible later extensions; no particular analysis library or expanded first-release scope has been selected.

## Open decisions

- Trustworthy reuse of completed inventory work and verification immediately before cleanup, including changed files and remounted shares.
- Platform-specific link and metadata support, including hard-link relationships and detection limits for references.
- Exact Windows release timing after assessing build and verification effort.
- Select and validate individual libraries or narrowly scoped upstream code reuse within Folderloom's own workflow; assess integration and performance before importing broad analysis dependencies.

## Implementation status

Read-only persistent inventory and the staged comparison foundation ([#4](https://github.com/mk11232/folderloom/issues/4)) are implemented in the Rust core and development CLI. Local SQLite stores metadata observations, staged BLAKE3 evidence, and progress across interruption, pause, and resume, with stale observations, incomplete reads, and failures kept visible. Validation uses synthetic fixtures; inventory completion does not establish content equality, and comparison evidence does not authorize copying or cleanup.

Folder suggestions and saved merge plans ([#5](https://github.com/mk11232/folderloom/issues/5)), the Tauri desktop interface, copying and destination verification, and separately approved cleanup remain future stages. Actual NAS behavior and Windows support remain unvalidated. See the [README](../README.md) for the current commands, validation coverage, and limitations.
