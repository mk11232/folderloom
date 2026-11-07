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

## Archive, restore, and permanent deletion — November 7, 2026

- 13 focused desktop job tests and 27 frontend tests passed. TypeScript, the production frontend build, desktop Clippy with warnings denied, and formatting passed.
- Job tests cover archive/restore across restart; deletion restricted to archived jobs; selected-job reset; preservation of other jobs, drafts, source bytes and copied output; invalid IDs; linked companion rejection; and both the active worker gate and an external engine writer lock.
- Built and installed the standalone macOS app, then verified its ad-hoc signature. In the native UI, archived an existing sample job, quit/reopened, restored it, quit/reopened again, and confirmed all three original jobs were Current. Inspected the final compact Open/archive/restore/delete controls and the named deletion confirmation. Keep job cancelled the confirmation and retained the job; the sample was restored afterward.
- Permanent deletion was exercised against temporary synthetic jobs in backend tests. No existing user job was permanently deleted through the native UI.
- Installed executable SHA-256: `f3c73aef1cc7c1437548f38289653416492da4b7425273cef73f509a3f72cf40`.

Kimi K3 High independently reviewed archive/restore and permanent deletion and reported no blocking issues. Its frontend tests and typecheck passed; its Rust adapter tests passed with the 26.5 SDK and default desktop features disabled. The full native build and installed-app checks above were performed separately with desktop features enabled.

## Resize and cancelled setup fixes — November 7, 2026

Reproduced the missing action column in the installed app at the previous 960 × 640 minimum, along with the inability to shrink further. Verified the rebuilt installed app at 720 × 500 with Open and Archive still visible, and observed the archived row’s Open, Restore, and Delete controls at a wider compact size. The user took over native interaction afterward; no further automated UI changes were made.

The two new cancellation serialization tests failed before implementation and passed afterward. All 29 frontend tests and 14 desktop job tests pass, including cancellation across restart, empty-draft suppression, preservation of started jobs and source bytes, and the active-worker gate. TypeScript, native bundle, signature verification, and desktop Clippy passed. Kimi K3 High’s text-only cancellation review found no blocking issues. Cancellation persistence was verified through the real backend tests, not claimed as a completed native cancel/restart exercise.

Installed executable SHA-256: `b7097194899e1497536cc41d505d9274ba088c636456c53a1f4222dbced2a1cf`.

## Action labels and warnings

Installed and visually checked the red-at-rest Delete control and the Restore/Delete explanation popovers using keyboard focus in the native app. Popovers also open on hover and render outside the scrolling table to avoid clipping. The permanent-delete popover and confirmation explicitly say the deleted job cannot be recovered, while source folders and copied files stay untouched. The final installed app exposes the requested Open job label. All 29 frontend tests and the full native bundle build passed; signature verification passed.

Latest installed executable SHA-256: `2ede7f47b392e18472a67c8ece37ba886b106e0f99e11e422f0822e8e90e442b`.
