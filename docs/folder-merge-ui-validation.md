# Folder arrangement UI validation

2026-11-07. UI-only change; core planning, revision approval and copying unchanged.

- Frontend tests: 35 passed, including conservative layout selection, unique separate-folder names, custom-layout reopening, exact destination details and revision consent.
- Production frontend and macOS Tauri app build passed using macOS 26.5 SDK. Installed app signature verified.
- Native installed app: overview, combine/separate choices and source checkboxes inspected.
- An old sample job refused planning because its root identity had changed. Created a fresh job using the same three sample paths; inventory completed with 502 entries.
- Selected only copy-1 with Keep each folder separate. Saved preview revision 1 showed 201 items (71 additions and 130 folders). Expanded a file and verified exact source and destination paths.
- Returned to Jobs and reopened. Separate layout and copy-1 selection persisted; copy-2 and copy-3 remained available and unchecked.
- No approval or copy execution performed in this UI check. No source/output files deleted.

Limits: native visual checks used the existing window size, not a full resize matrix. Matching files is an inspection view in an existing job, not standalone duplicate cleanup. Tree paging limits rendered children but the snapshot still loads in memory; this is not a large-NAS scalability claim. Claude reviewed the layout helpers and preview flow; its saved-selection finding was fixed and verified in the native reopening check.

## Compact combine follow-up

Combine is the default presentation; alternate arrangements use a disclosure and selector. The repeated workspace brand, large destination panel, metric blocks and stacked search labels were compacted. Full destination paths remain available through Full path.

35 frontend tests and macOS production build passed. Native sample job changed from separate to combine with all three sources selected; saved revision 2 contained 219 planned items and reopened in the rebuilt app. Visually confirmed initial review shows destination tree rows at the existing window size. No copying performed.

## File-location outcomes (2026-11-08)

Default plan preview now lists files with relative destination paths, inclusion controls, sources, and copy outcomes visible. Saved plan hashes identify matching contents across distinct output paths and differing contents under the same source filename. Up to five example locations are shown per relationship. This is comparison of saved plan evidence, not a fresh scan or automatic cross-path deduplication. The optional folder view retains folder-only inclusion semantics.

38 frontend tests passed, including photos/photo1.jpg and photos/photos/photo1.jpg sharing a hash while photos/ph/photo1.jpg has different contents. The test asserts three destination locations remain. Production native build passed. Installed demo reopened revision 2 and displayed same-name/different-content outcomes and inclusion without opening details. No copying executed.

User review rejected stacked file blocks. Final default view is a data table: Include, File/destination location, Sources, Content match, What happens. Named source roots are visible, and same/different relationships link to numbered rows. Native installed screenshot confirmed compact rows; 38 tests pass. No copy-engine changes or source writes.

## Related-file comparison (2026-11-08)

Compare now keeps the selected file and all related saved-plan files together: matching hashes across locations, or the same source filename with different hashes. The selected row stays first and highlighted. Relationships use item IDs rather than matching escaped destination strings. Show all files exits comparison. Search and outcome filters are disabled during comparison to avoid silently omitting a version. Sorting by path or outcome remains available.

Existing destination items now distinguish verification from exclusion: excluding them leaves them unchanged. Toolbar controls are consolidated and long cells wrap. 40 frontend tests passed, including the three-photo example, colliding display paths and existing-file outcomes. Native production build passed. Live app: searched Decline Rep.docx, selected Compare on the original, observed all three versions with Selected file/Different contents and their source/output paths. Final installed rendering verified complete labels without clipping. No copies or file deletions.
