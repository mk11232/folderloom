# Inventory milestone validation

Scope: issue #2, under product specification #1. The confirmed test seam is the public Rust core job interface, real temporary filesystem trees and SQLite, plus the development runner for process restarts. No real NAS or client data was inspected.

## Observed TDD cycles

The following failures were observed before their corresponding implementations, then the focused tests passed:

| Slice | Red evidence | Green behavior |
| --- | --- | --- |
| Persistent inventory | Unresolved public `Job`, `Options`, and `State` imports | Scan and reopen hidden, empty, and unique entries |
| Source storage boundary | `storage_in_source_or_its_alias_is_rejected_before_any_write` failed because creation succeeded | Reject direct and symlink-aliased storage under a source before creating files |
| Pause and refresh | Missing public `pause` and `resume` operations | Paused state persists; resume starts generation 2, retains removed observations as stale, and discovers additions |
| Root disappearance | `root_loss_after_checkpoint_and_replacement_never_claim_full_coverage` failed because previously discovered children remained current | Root loss invalidates descendant observations; replacement identity remains an explicit failure |
| Process lifecycle | Runner produced no `state=Running` checkpoint | Kill at an incomplete-directory checkpoint, reopen, pause another process, and resume without missing or duplicate entries |
| Replaced child directory | Expected `FinishedWithErrors`, received `Complete` | Persist directory identity/change signals and reject replacement between discovery and enumeration |
| Duplicate roots | A rejected duplicate selection left a partial database | Validate canonical duplicates before any database creation |

Additional public-interface regressions cover unreadable-directory retry, historical failures, visible empty exclusions, symlink/broken-link/cycle targets, source content/name/mtime preservation, database relocation and sidecar aliases, and single-writer/read-only-reader behavior.

## Checks

Local macOS validation: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked`. The workspace currently has 12 core integration tests and one subprocess integration test. CI repeats these checks on macOS and Linux. Remote CI results are separate from this local record.

The local APFS filesystem rejects invalid UTF-8 names with `Illegal byte sequence`; Linux CI executes that part of the native-name regression. Permission denial is exercised as an ordinary user and explicitly skipped when the runner can bypass permissions. Windows, network disconnection behavior on actual NAS mounts, performance at collection scale, and adversarial concurrent filesystem mutation remain unvalidated. No hashing, copying, deletion, desktop UI, or migration is implemented.
