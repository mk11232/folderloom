# Folderloom session handoff — 2026-11-09

## Shipped state

PR #17 is merged at `7804e348c3ac15723d96bfe242878793a6897903` (following UI PRs #13–#16). The updated native demo was built, installed at `/Users/mark/Applications/Folderloom Demo.app`, reopened and checked. The main checkout was synchronized to the merge before this handoff.

The default preview is a compact data table with inclusion, destination location, named sources, content match and copy outcome. Compare brings the selected file and all related saved-plan files together (same hash or same source filename), highlights the selected row, and pages results. Show all files exits comparison. Search/outcome filters are disabled during comparison so they cannot silently omit a version. Sorting remains available. Existing destination files say “Already here — verify only” or, when excluded, “Already here — unchanged.” Rebuilding a plan resets comparison state. Long cells wrap; toolbar space is reduced.

## Required behavior and design direction

Prefer comparable table rows over stacked per-file paragraphs. Show where contributors are, whether contents match, and what happens at the destination. Do not substitute unexplained counts for locations. Preserve sources through verified copying.

`photos/photo1.jpg` and `photos/photos/photo1.jpg` can have identical hashes while `photos/ph/photo1.jpg` differs. Preserving the folder structure retains all three destination locations. Same-target/same-hash contributors combine; different contents at the same target are preserved with renamed outputs. This is not automatic cross-path deduplication or in-place duplicate deletion.

## Evidence

40 frontend tests passed. The native production build passed using the macOS 26.5 SDK. Installed demo comparison showed the original and two different versions of `Decline Rep.docx`, named sources and destination paths, with complete labels after the wrapping fix. No copy or deletion was executed during UI checks. PR #17 macOS/Linux checks passed; one Linux job needed a rerun after an existing Rust test encountered a temporary `WouldBlock` resource error.

See `folder-merge-ui-research.md` and `folder-merge-ui-validation.md` for research and detailed verification. Competitor references include Czkawka/Krokiet, dupeGuru and WinMerge; published guidance includes Nielsen Norman Group heuristics/progressive disclosure and W3C form labels.

## Remaining work and limits

- Optional folder view still has duplicated branch/detail presentation and folder-only checkbox semantics; it does not recursively include children.
- Preview includes plan-involved items, not an enumeration of every existing destination file.
- Same-basename source roots may remain ambiguous; full paths are available in tooltips.
- Large-NAS snapshot memory/scalability and a complete window-resize matrix remain unverified.
- Default relationship summaries are bounded; Compare retrieves every related plan item and pages the rows.

## Resume

Work in the existing checkout; avoid creating worktrees unnecessarily. All shell commands must use `rtk`. Native builds require:

```sh
rtk proxy env SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk npm run tauri build -- --bundles app
```

Run from `apps/desktop`. The default SDK 27 previously produced linker problems. Frontend validation is `rtk proxy npm test` from that directory. Preserve saved jobs and sources during checks. Confirm live git/app state before resuming; this handoff is dated evidence.
