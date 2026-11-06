# Folderloom

Folderloom is a planned desktop application for consolidating overlapping folder collections into a chosen primary destination while preserving meaningful structure, differing file contents, and source history.

**Status: read-only inventory, staged comparison, and a macOS desktop demo.** The Rust core, development CLI, and Tauri desktop interface scan explicitly selected folders into local SQLite, compare file contents, reopen saved observations, and resume interrupted jobs. Folder mappings, merge plans, copying, destination verification, and cleanup remain future work.

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

## Run the macOS desktop demo

The desktop interface uses the same public Rust `Job` operations as the CLI. It bundles its React interface inside the application; no hosted backend, external fonts, or source-content upload is involved. The demo exposes inventory and comparison only.

From `apps/desktop`, using Node 22.12+ (Node 26 was used during development), npm, Rust, and the macOS native build prerequisites:

```sh
npm ci
npm run typecheck
npm run tauri dev
```

Build a standalone application with `npm run tauri build -- --bundles app`. Open `target/release/bundle/macos/Folderloom.app` in Finder. `npm run build` checks TypeScript and produces the bundled frontend assets; `npm run dev` alone starts the frontend development server and cannot perform native job operations.

The app reads an optional private `demo.json` from its application data directory (Tauri identifier `com.folderloom.demo`). Its shape is `{ "title": "Folder comparison", "description": "Local sample", "roots": [{ "label": "Copy 1", "path": "..." }] }`; supply actual local absolute paths privately, never commit them. When the configuration is absent, **Add folder** opens the native directory picker. Select sources and choose **Scan selected folders** to create a new saved job. Each new job retains the previous database on disk; the app automatically reopens the current saved job, and **Reopen saved job** reloads its saved state. This first interface does not include a historical job browser.

After inventory finishes, **Compare contents** runs size screening, edge samples, and full hashing through the core. **Pause** cooperatively stops after a saved checkpoint; **Resume scan** or **Resume comparison** continues the corresponding work. **Re-scan** starts a new validation generation for the current job and invalidates old comparison evidence. The source checkboxes apply to a new scan; re-scanning an existing job uses that job's saved roots.

Search paths with Command-F, filter by source or evidence, and select a placement to inspect the full root/relative path, recorded modification time, sample and full hashes, and other current full-hash placements. Table pagination keeps rendering bounded; the current core read API still loads the full inventory. Unique-size and unique-sample candidates remain visible. The gaps view includes inventory failures, root gaps, failed content reads, and their generations; stale entries can be inspected through **Needs attention**. Samples do not prove equality, and saved hashes do not authorize copying or cleanup.

Private source files, manifests, provenance, databases, and screenshots belong outside the public repository. Development fixtures must be synthetic. A developer's bounded local copy of a NAS sample is setup only; it does not establish native app support for scanning a live NAS or for Windows. Packaged macOS interaction and persistence validation are recorded separately from frontend type/build checks.

## Run the development CLI

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

Exit codes are 0 for complete, cooperative pause, or a successful read; 2 for a scan or comparison finished with errors; and 1 for invalid commands or storage errors. `status` reports separate inventory and comparison progress, current-generation inventory failure counts, and all historical inventory failures; `inventory` adds entries. Paths use escaped native-byte debug formatting to distinguish otherwise ambiguous names. `--` ends option parsing for source names beginning with `--`.

Every resume starts a new validation generation, marks prior observations stale, revalidates root device/inode identity, and repeats discovery from the roots. Observations are updated by root and native relative path, so repeats do not duplicate entries. Removed or unavailable entries remain visible as stale, and historical failures remain available. A replacement root requires a new job. Status and inventory show saved observations; they do not claim a newly verified scan.

Directory work commits in bounded child batches. Reaching EOF and validating the directory's identity/change signals is required before marking enumeration complete. Interrupted or unreadable directories remain incomplete. Resume deliberately re-enumerates previously completed directories too: this costs scan time but detects source changes without trusting old directory offsets. `Complete` means metadata discovery completed without detected failures or stale entries in that generation. `FinishedWithErrors` means attempts ended with gaps, stale observations, or failures. Neither state establishes content equality or safe copying/deletion.

## Compare saved file placements

Run comparison after inventory reaches `Complete` or `FinishedWithErrors`. Use synthetic sources only.

```sh
cargo run -p folderloom-cli -- compare "$HOME/Library/Application Support/Folderloom/demo.sqlite"
cargo run -p folderloom-cli -- comparison "$HOME/Library/Application Support/Folderloom/demo.sqlite"
cargo run -p folderloom-cli -- compare "$HOME/Library/Application Support/Folderloom/demo.sqlite" --byte-budget 65536 --interactive
```

`compare DATABASE` starts or resumes comparison without starting a new inventory generation. `--byte-budget N` bounds source-content bytes read per checkpoint (default 1 MiB; the core also caps each step). Interactive mode commits a checkpoint before accepting `next` or `pause`; EOF pauses too. Repeating `compare` resumes completed work and restarts an interrupted file's unfinished hash. A saved `Running` state can mean the previous process ended abruptly. `comparison DATABASE` opens a read-only observer, including while a writer is running.

Comparison screens regular-file placements by size, then BLAKE3 edge samples (`edge-v1-4096`, first and last 4096 bytes without overlap), then full BLAKE3 hashes for matching sample candidates. A sample never proves equality. `UniqueSize` and `UniqueSample` mean the file left the candidate pipeline; its placement stays in the inventory. Identical contents at different paths remain separate placements. Full hashes are saved comparison evidence, not authority for copying or deletion.

Saved comparison output distinguishes the sample hash from the full hash, identifies the algorithm and sample format, and shows generation, stage, bytes read, stale evidence, and failures for each file record. Progress separates pending work, completed full hashes, unique candidates, and failures. `failures` counts current-generation file and root gaps together; `root_failures` is the root-only subset. `status`, `inventory`, and `comparison` show saved root-gap messages with root IDs, generations, and stale flags alongside the root ID/path mapping. Root gaps do not fabricate file records or alter inventory failures. In-progress samples and full reads cannot stand in for completed evidence. The byte counter in progress is cumulative, including reads repeated after interruption; a file's counter describes its current attempt.

Comparison independently validates every selected root at preparation and completion, including empty and symlink-only roots. It checks root identity, directory type, read access, and recorded root-directory change signals where available. File reads also check root identity, path type, and recorded file/ancestor change signals before and after reads. Missing, changed, unreadable, or link-replaced roots and files create explicit gaps while unrelated work continues. A root-directory change leaves unchanged file hashes available as saved evidence, but the comparison ends `FinishedWithErrors`. The first root gap in a generation remains saved even if access later recovers; a fresh inventory generation is required to establish new observations. Reopening comparison also validates cached evidence; a read-only status command reports the saved state without inspecting sources. Inventory `resume` starts a new generation and invalidates old comparison evidence; rerun `compare` after the new scan finishes. Older-generation evidence remains visible as stale. Read-only `status`, `inventory`, and `comparison` can open legacy milestone-1 databases without modifying them. The `compare` and `resume` writers apply an additive schema v1-to-v2 upgrade. This upgrade is one-way for that job database: the milestone-1 binary rejects the upgraded version. Old observations need a fresh inventory `resume` before comparison has the required identity evidence.

## Validation and limits

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Tests use the public core job interface and real temporary trees/SQLite. A subprocess test kills a runner after observing a durable checkpoint in an incomplete directory, reopens status, cooperatively pauses another runner, and resumes to a complete inventory. Comparison subprocess tests exercise durable incomplete full hashes, abrupt termination, cooperative pause, reopened saved evidence, and resume. See [comparison validation](docs/comparison-validation.md) for evidence and limits. Other tests cover source content/name/mtime preservation, native paths, symlink targets, exclusions, stale observations, root replacement, unreadable directories, storage aliases, and writer exclusion. Linux CI exercises invalid UTF-8 names because the local macOS APFS filesystem rejects their creation. Permission-denial tests require a non-root user. CI runs formatting, linting, and tests on macOS and Linux; Windows and actual NAS behavior are not validated or supported in this milestone.

Inventory discovery reads directory entries, link targets, and metadata only. Comparison opens candidate regular files for read-only content hashing. Symbolic links are recorded and not intentionally traversed. Selected roots are canonicalized at creation. Storage containment checks resolve canonical/symlink aliases before creating database, journal, or lock files; unsafe hard-linked databases and sidecars are rejected. Keep the local database and its sidecars together. Normal SQLite hot journals are recoverable by `resume`; a read-only status request may require resume first if recovery is pending.

Inventory is an observation over time, not an atomic filesystem snapshot. Source metadata can change after observation; device/inode identities and timestamps are change signals, not content verification or a guarantee against inode reuse, remount ambiguity, or adversarial path replacement races. Use stable synthetic sources for this milestone. Filesystem-managed access times may change. Root creation requires accessible existing directory roots; unavailable roots encountered later produce explicit failures. Inventories and comparison records are currently loaded in full by the read interface. Only the milestone-1 to comparison-schema migration is supported; cross-machine portability is not implemented, and unknown formats are rejected by version.

## Data and contributions

Use synthetic fixtures for development. Keep real files, client information, source inventories, job databases, and private handoff notes outside the public repository. Contributions should preserve the separation between review, copying, verification, and source cleanup.

## License

Folderloom's original code and documentation are licensed under the [MIT License](LICENSE). Third-party libraries and any future reused code retain their respective licenses and notices.
