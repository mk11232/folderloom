# Saved jobs and wizard validation — 2026.11.3

This build changes desktop job navigation and setup. The underlying copy-only consolidation rules remain unchanged.

## Local checks

- 20 desktop adapter tests passed: 9 jobs, 9 prior workflow, 2 consolidation.
- 26 frontend tests and TypeScript checks passed.
- Workspace Clippy with warnings denied and formatting passed.
- Production bundle and installed native verification are recorded below.

## Behavioral checks

The desktop host tests use real temporary directories and SQLite jobs. They cover multiple named jobs, selecting an older saved plan after reopening the host, durable source removal in a draft without changing files or historical jobs, primary-folder exclusion, disjoint app-data sample paths, invalid IDs and linked database rejection, unreadable job rows, metadata errors, and historical hot-journal recovery.

An exclusive SQLite lock holds an active worker deterministically while the test attempts job switching, job creation and draft mutation. Those operations are rejected until the worker finishes, avoiding a timing-dependent test on a tiny scan. Comparison pauses/errors and outdated plan generations have distinct catalog states.

Frontend tests exercise the jobs table and source-removal controls, stage labels, setup validation, source/destination overlap, resume-stage selection, ordered draft saves and loading saved jobs independently of a broken draft.

## Installed native verification

Built and installed 2026.11.3 and verified its ad-hoc signature. Launch opened the Jobs table with both existing local jobs. Created a named setup, added and removed a source selection, quit the app, reopened Continue setup, and confirmed the removed source stayed absent.

Opened a retained job at its saved plan stage, used it as the starting point for a new job, changed its name, removed one of four source selections, reviewed the existing populated primary destination and started the scan. Jobs then showed three independent records: the new three-source job with 502 entries and both original four-source records with 574 entries each. The new job retained its selected destination, and the consumed draft cleared after successful creation.

An independent filesystem check confirmed all 262 baseline source files retained their content hashes, sizes and modification times, and the prior destination still contained its 92 files. No original NAS contents were changed. Private file paths and screenshots are excluded from this record.

Installed executable SHA-256: `5d70e0e1187d2d3d0f3a8e39acd273ad49146940c802ffef86694c2f11d52aa3`.

## Validation limits

This is a local, ad-hoc signed macOS development build. CI also checks Linux. Mounted-NAS compatibility, Windows, automatic folder matching, source cleanup and malware scanning remain outside this build. Job metadata and drafts stay local; private samples and screenshots are excluded from the repository.
