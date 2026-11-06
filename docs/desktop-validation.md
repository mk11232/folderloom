# Desktop validation

The Tauri host uses the same public Rust `Job` interface as the development CLI. The desktop adapter is a public seam tested with synthetic temporary source trees, real SQLite, and a fresh test process. No private manifests, source files, screenshots, inventories, or job databases are test fixtures.

## Automated checks

`cargo test --workspace --locked` includes these adapter scenarios:

- Inventory, staged full-content comparison, distinct file placements, saved current-job reopening, and unchanged source names/content.
- A desktop pause request releasing the worker, allowing legitimate early completion; deterministic unfinished pause/reopen/resume through the public Job seam without thread-scheduling assumptions.
- Recovery after process death leaves a real SQLite hot journal: read-only inspection fails, the desktop retains the validated saved job ID, and explicit Recover / Resume completes through the guarded core writer.
- Native parent placement keys, including escaped display names, so tree hierarchy does not depend on parsing display paths.
- Unavailable source roots producing comparison root gaps and stale inventory on rescan.
- A fresh process reopening saved inventory and comparison evidence.
- Reopen in the same window reloading durable changes made through the public Job interface while idle.
- Raw valid-UTF-8 source paths kept separate from escaped display paths when selecting saved roots again.
- Distinct invalid-UTF-8 placement keys and escaped display paths on Linux. macOS APFS rejects the synthetic invalid filename, so that filesystem-specific fixture is Linux-only.

The lightweight adapter command is `cargo test -p folderloom-desktop --no-default-features --test workflow`. Native checks use the default `desktop` feature; packaged builds enable `custom-protocol` to embed frontend assets. Frontend validation is `npm ci`, `npm test`, and `npm run build` from `apps/desktop`. CI installs Linux WebKit/GTK dependencies and runs frontend build and tree tests, formatting, workspace Clippy, and all workspace tests on Linux and macOS.

## Private local setup and runtime

The installed app resolves its data directory through Tauri `app_data_dir()` for identifier `com.folderloom.demo`; no developer-specific path or environment variable is required. The optional `demo.json` contains `title`, `description`, and `roots` objects with `label` and `path`. The configuration loads on startup without starting a scan. Explicitly chosen folders can also be selected with the native folder picker. Never commit actual source paths or private configuration.

Job databases live under the data directory's `jobs` subdirectory. An atomically replaced `current-job` pointer chooses the job to reopen. Reopening reads saved observations; Scan/Resume explicitly revalidates sources. Comparison errors may require re-scan before retry, as explained by the core's saved failure. An abrupt app exit can leave saved state `running` without an active worker: use Resume. Pause requests take effect after the current bounded core checkpoint; a blocked filesystem operation cannot be forcibly interrupted safely.

If saved observations cannot be read, the validated current job remains selected and **Recover / Resume** invokes the core's guarded inventory resume. This can recover a normal SQLite hot journal and starts a fresh source-validation generation; storage-safety and source checks still apply. The control state (selected job, worker mode, pause, recovery, error) is typed independently of the inventory JSON payload.

A single reserved background worker owns writes; commands return cached checkpoint snapshots to keep the window responsive. The adapter and UI expose no source copy, merge, rename, or delete operations. Full hashes are observations, not authority to clean up sources. Source/database path safety and cross-process job locking remain enforced by the core.

## Evidence boundaries

A native packaged UI exercise and any private local sample review are separate from automated synthetic tests. Building on macOS does not establish Windows support or real NAS scanning performance. The current core is Unix-specific. Whole inventory snapshots are appropriate for this bounded demo; very large collections will need paging and measured refresh costs before production scale claims.

## Observed packaged macOS workflow

Review-fix regression evidence at `b392d57`: the synthetic hot-journal test first failed because desktop bootstrap returned a null saved job ID, then passed after the recovery fix. The parent-key test first failed on a missing parent key, then passed after native parent keys were included. Formatting, workspace Clippy with warnings denied, all 40 macOS integration tests (including seven desktop adapter tests), and frontend type/production build passed. The Linux escaped-name fixture uses an explicit native byte component to preserve its backslash while avoiding Clippy's cross-platform absolute-path interpretation; Linux execution remains a CI check. These checks are separate from the earlier native interaction described below; that run does not establish native interaction coverage for later review fixes.

The release app built from runtime commit `1e27e48` was installed in the user's Applications folder and launched without a development server. Through the native window, a deliberately selected private local sample was scanned and compared to completion with zero reported gaps. The full-hash filter and placement detail panel displayed the corresponding saved evidence and matching locations. Quitting the application and launching it again reopened the same job identifier, inventory generation, entry count, full-hash count and unique-candidate count.

The sample was copied from a bounded selection of NAS folders as separately authorized setup. Each destination file was SHA-256 checked against the source bytes read; source identity, size and modification-time checks stayed unchanged. Local copies preserve contents, relative paths and modification times, but use private permissions rather than NAS flags or ACLs. This is evidence for running the app on local copies, not for direct NAS scanning or a production copy/verification engine. Sample files, full source paths, provenance and screenshots remain private and are not repository artifacts.


## Expandable desktop and packaged resources

Runtime `fd990ba` (CalVer `2026.11.1`) passed 40 Rust integration tests, six synthetic tree tests, frontend production build, formatting, strict Clippy, and deterministic license checks. A clean Tauri build with explicit ad-hoc signing passed `codesign --verify --deep --strict`. The installed package contained the expected version, new ICNS artwork, and all 16 license resource files; those files matched the generated sources. This is development signing, not Developer ID signing or notarization.

The installed app reopened the user's existing completed job with the same identifier, generation and saved counts. Native UI checks confirmed the new woven-folder branding, expanded source roots with collapsed descendants, pointer expansion and keyboard expansion of a nested folder to reveal its file. Searching for a file displayed four matching placements and their ancestor context. Collapsing a branch while searching reduced visible rows while preserving the four-match count. Choosing a matching placement in another source cleared the search, restored ordinary expansion, opened its ancestors and selected the corresponding tree row. This test used saved observations from the private local copies and did not rescan the NAS.

Replacing an older generated application bundle by overlay initially left its obsolete icon file behind. Strict signature verification identified that extra file; removing only the obsolete generated resource restored verification. Future installs should replace the generated app bundle cleanly rather than overlaying renamed resources. Job databases and source files are outside the app bundle and were retained.
