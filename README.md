# Folderloom

Folderloom is a planned desktop application for consolidating overlapping folder collections into a chosen primary destination while preserving meaningful structure, differing file contents, and source history.

**Status: read-only persistent inventory.** The Rust core and development CLI scan explicitly selected folders into local SQLite, reopen saved observations, and resume interrupted jobs. Comparison, the desktop interface, copying, verification of file contents, and cleanup remain future work.

## Intended workflow

1. Select multiple source roots and a primary destination.
2. Build a saved inventory with resumable progress and staged content comparison.
3. Review suggested folder mappings, new files, and conflicts in a merge plan.
4. Copy approved additions and verify their contents and destination placements.
5. Separately review whether to retain originals, move them to recovery, or remove verified source copies.

Preserve selected snapshots or dependent folder collections separately when their internal structure must remain intact. Identical contents at different meaningful paths retain their placements unless the user explicitly chooses otherwise.

## Design

- [Product specification](docs/SPEC.md)
- [Full product specification issue](https://github.com/mk11232/folderloom/issues/1)
- [Inventory milestone issue](https://github.com/mk11232/folderloom/issues/2)
- [Architecture and delivery sequence](docs/DESIGN.md)
- [Requirements record](docs/requirements.md)
- [Domain glossary](GLOSSARY.md)
- [Desktop architecture decision](docs/adr/0001-desktop-shell-and-reusable-rust-core.md)
- [Czkawka evaluation](docs/czkawka-evaluation.md)

The accepted architecture is a reusable Rust engine, local SQLite job state, and a Tauri desktop interface using React and TypeScript. macOS with local and OS-connected NAS folders is the first target; Windows support will follow or join the initial release when validated.

Folderloom uses its own consolidation workflow and will build on established libraries and techniques from projects such as Czkawka. It is not a fork of Czkawka, and no upstream implementation has been copied into this project.

## Run the inventory CLI

Use a stable Rust toolchain and C build tools on macOS or Linux. SQLite is bundled. Create the database parent directory outside the source folders and repository. The database must be new for `scan`; `resume` opens an existing job.

```sh
mkdir -p "$HOME/Library/Application Support/Folderloom"
cargo run -p folderloom-cli -- scan "$HOME/Library/Application Support/Folderloom/demo.sqlite" /path/to/synthetic-source
cargo run -p folderloom-cli -- status "$HOME/Library/Application Support/Folderloom/demo.sqlite"
cargo run -p folderloom-cli -- inventory "$HOME/Library/Application Support/Folderloom/demo.sqlite"
cargo run -p folderloom-cli -- resume "$HOME/Library/Application Support/Folderloom/demo.sqlite"
```

`scan DATABASE ROOT...` accepts multiple roots, includes hidden and unique files, and defaults to `--empty-folders include`. `--empty-folders ignore` keeps physically empty directories in the inventory with `excluded=true`; it does not remove them. Parent directories containing empty children are not themselves physically empty. Overlapping roots retain separate placements under their root IDs; duplicate canonical roots are rejected.

`scan` and `resume` accept `--batch-size N` (default 256, core maximum 4096). Add `--interactive` to commit one batch at a time: enter `next` to continue or `pause` to save and stop. EOF also pauses. An abrupt termination may leave `state=Running`; that describes saved state, not proof a process is still alive. Resume is safe after either pause or termination. A writer lock rejects a second runner while allowing status readers.

Exit codes are 0 for complete, cooperative pause, or a successful read; 2 for a scan finished with errors; and 1 for invalid commands or storage errors. `status` reports current-generation failure counts and all historical failures; `inventory` adds entries. Paths use escaped native-byte debug formatting to distinguish otherwise ambiguous names. `--` ends option parsing for source names beginning with `--`.

Every resume starts a new validation generation, marks prior observations stale, revalidates root device/inode identity, and repeats discovery from the roots. Observations are updated by root and native relative path, so repeats do not duplicate entries. Removed or unavailable entries remain visible as stale, and historical failures remain available. A replacement root requires a new job. Status and inventory show saved observations; they do not claim a newly verified scan.

Directory work commits in bounded child batches. Reaching EOF and validating the directory's identity/change signals is required before marking enumeration complete. Interrupted or unreadable directories remain incomplete. Resume deliberately re-enumerates previously completed directories too: this costs scan time but detects source changes without trusting old directory offsets. `Complete` means metadata discovery completed without detected failures or stale entries in that generation. `FinishedWithErrors` means attempts ended with gaps, stale observations, or failures. Neither state establishes content equality or safe copying/deletion.

## Validation and limits

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Tests use the public core job interface and real temporary trees/SQLite. A subprocess test kills a runner after observing a durable checkpoint in an incomplete directory, reopens status, cooperatively pauses another runner, and resumes to a complete inventory. Other tests cover source content/name/mtime preservation, native paths, symlink targets, exclusions, stale observations, root replacement, unreadable directories, storage aliases, and writer exclusion. Linux CI exercises invalid UTF-8 names because the local macOS APFS filesystem rejects their creation. Permission-denial tests require a non-root user. CI runs formatting, linting, and tests on macOS and Linux; Windows and actual NAS behavior are not validated or supported in this milestone.

The scanner reads directory entries, link targets, and metadata only; it does not open file contents. Symbolic links are recorded and not intentionally traversed. Selected roots are canonicalized at creation. Storage containment checks resolve canonical/symlink aliases before creating database, journal, or lock files; unsafe hard-linked databases and sidecars are rejected. Keep the local database and its sidecars together. Normal SQLite hot journals are recoverable by `resume`; a read-only status request may require resume first if recovery is pending.

Inventory is an observation over time, not an atomic filesystem snapshot. Source metadata can change after observation; device/inode identities and timestamps are change signals, not content verification or a guarantee against inode reuse, remount ambiguity, or adversarial path replacement races. Use stable synthetic sources for this milestone. Filesystem-managed access times may change. Root creation requires accessible existing directory roots; unavailable roots encountered later produce explicit failures. Inventories are currently loaded in full by the read interface, schema migration and cross-machine portability are not implemented, and new formats are rejected by version.

## Data and contributions

Use synthetic fixtures for development. Keep real files, client information, source inventories, job databases, and private handoff notes outside the public repository. Contributions should preserve the separation between review, copying, verification, and source cleanup.

## License

Folderloom's original code and documentation are licensed under the [MIT License](LICENSE). Third-party libraries and any future reused code retain their respective licenses and notices.
