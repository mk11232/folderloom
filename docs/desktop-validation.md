# Desktop validation

The Tauri host uses the same public Rust `Job` interface as the development CLI. The desktop adapter is a public seam tested with synthetic temporary source trees, real SQLite, and a fresh test process. No private manifests, source files, screenshots, inventories, or job databases are test fixtures.

## Automated checks

`cargo test --workspace --locked` includes these adapter scenarios:

- Inventory, staged full-content comparison, distinct file placements, saved current-job reopening, and unchanged source names/content.
- Cooperative pause at a bounded checkpoint, rejecting overlapping worker requests, reopening a paused job, and explicit resume.
- Unavailable source roots producing comparison root gaps and stale inventory on rescan.
- A fresh process reopening saved inventory and comparison evidence.
- Reopen in the same window reloading durable changes made through the public Job interface while idle.
- Raw valid-UTF-8 source paths kept separate from escaped display paths when selecting saved roots again.
- Distinct invalid-UTF-8 placement keys and escaped display paths on Linux. macOS APFS rejects the synthetic invalid filename, so that filesystem-specific fixture is Linux-only.

The lightweight adapter command is `cargo test -p folderloom-desktop --no-default-features --test workflow`. Native checks use the default `desktop` feature; packaged builds enable `custom-protocol` to embed frontend assets. Frontend validation is `npm ci && npm run build` from `apps/desktop`. CI installs Linux WebKit/GTK dependencies and runs frontend build, formatting, workspace Clippy, and all workspace tests on Linux and macOS.

## Private local setup and runtime

The installed app resolves its data directory through Tauri `app_data_dir()` for identifier `com.folderloom.demo`; no developer-specific path or environment variable is required. The optional `demo.json` contains `title`, `description`, and `roots` objects with `label` and `path`. The configuration loads on startup without starting a scan. Explicitly chosen folders can also be selected with the native folder picker. Never commit actual source paths or private configuration.

Job databases live under the data directory's `jobs` subdirectory. An atomically replaced `current-job` pointer chooses the job to reopen. Reopening reads saved observations; Scan/Resume explicitly revalidates sources. Comparison errors may require re-scan before retry, as explained by the core's saved failure. An abrupt app exit can leave saved state `running` without an active worker: use Resume. Pause requests take effect after the current bounded core checkpoint; a blocked filesystem operation cannot be forcibly interrupted safely.

A single reserved background worker owns writes; commands return cached checkpoint snapshots to keep the window responsive. The adapter and UI expose no source copy, merge, rename, or delete operations. Full hashes are observations, not authority to clean up sources. Source/database path safety and cross-process job locking remain enforced by the core.

## Evidence boundaries

A native packaged UI exercise and any private local sample review are separate from automated synthetic tests. Building on macOS does not establish Windows support or real NAS scanning performance. The current core is Unix-specific. Whole inventory snapshots are appropriate for this bounded demo; very large collections will need paging and measured refresh costs before production scale claims.

## Observed packaged macOS workflow

The release app built from runtime commit `1e27e48` was installed in the user's Applications folder and launched without a development server. Through the native window, a deliberately selected private local sample was scanned and compared to completion with zero reported gaps. The full-hash filter and placement detail panel displayed the corresponding saved evidence and matching locations. Quitting the application and launching it again reopened the same job identifier, inventory generation, entry count, full-hash count and unique-candidate count.

The sample was copied from a bounded selection of NAS folders as separately authorized setup. Each destination file was SHA-256 checked against the source bytes read; source identity, size and modification-time checks stayed unchanged. Local copies preserve contents, relative paths and modification times, but use private permissions rather than NAS flags or ACLs. This is evidence for running the app on local copies, not for direct NAS scanning or a production copy/verification engine. Sample files, full source paths, provenance and screenshots remain private and are not repository artifacts.
