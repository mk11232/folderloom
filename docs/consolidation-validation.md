# Consolidation validation — 2026.11.2

Validated on macOS on November 6, 2026. This is a locally built, ad-hoc signed application, not a notarized public release.

## Automated verification

- `cargo test --workspace --locked`: 59 integration tests passed across inventory, comparison, consolidation, CLI and desktop host.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- Desktop frontend tests: 18 passed, including destination visibility and primary-source exclusion. Production bundle build passed.
- Independent review found no remaining must-fix findings in the reviewed copy-only workflow.

The core tests exercise exact-revision approval, durable exclusions, pause/reopen, source and destination changes, non-overwrite behavior, retained distinct placements, existing populated destinations, symlink traversal, macOS file metadata and exact quarantine preservation, and case/Unicode filename collisions. A subprocess is terminated during a 128 MiB staged transfer; a fresh process resumes and verifies one complete destination file without staging leftovers.

## Installed app exercise

Used four existing local sample copies; no original NAS contents were changed. Through the installed native app: selected a destination, mapped all four roots, built the plan, excluded an item, reopened the saved job and confirmed the exclusion, included it again, approved revision 3 and copied its additions. Closed and relaunched the app and reopened Results.

Saved results showed 92 verified file additions, 130 verified directory placements (including the destination root), zero exceptions and zero pending items. Independent read-only SQLite and filesystem verification confirmed all 262 source file contributions match the 92 output files using SHA-256. All 262 source files retained their baseline content hash, size and modification time. No staging files remained. Private sample filenames, paths and screenshots are intentionally not included here.

The final installed build also exercised the always-visible destination selector: choosing a listed source as primary deselected and disabled its incoming mapping, retained the other three mappings, and disabled copying the old plan until rebuilding. Results continued to show the saved destination and completed revision. The latest layered artwork was verified against the installed ICNS resource. Installed executable SHA-256: `2dc09667121f0f75f692522da9903147a427c41634ff83defd6858c7b76a2f14`.

## Limits

This verifies a copy-only consolidation workflow with manually selected mappings. It does not establish every NAS filesystem's compatibility, Windows support, automatic folder matching, immutable backups, source cleanup/recovery or malware scanning. Directory metadata is not copied and is disclosed per item. Pause checkpoints occur between files. Existing target files are never overwritten; incompatible or changed items are held or fail visibly.
