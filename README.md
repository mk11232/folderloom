<img src="apps/desktop/src/assets/folderloom.png" alt="A folder being woven on a loom" width="112" />

# Folderloom

A desktop workspace for making sense of overlapping folder collections. Open several folders, explore their files in one view, and compare contents before deciding how to consolidate them.

**2026.11.1 · macOS development demo · read-only**

The graphical app can scan, compare, pause, resume, and reopen a saved job. Folder mapping, merge plans, copying, and cleanup are still being built. The current app leaves source files in place.

## Explore your folders

1. Open **Folderloom Demo.app** and choose **Add folder** for each collection you want to inspect.
2. Select the sources and choose **Scan selected folders**. Folderloom saves the inventory as it works.
3. Expand a folder to see its subfolders and files. Use **Expand all**, **Collapse all**, or search with **Command-F**. Search retains parent folders so every result has context.
4. Choose **Compare contents**. Filter for full-hash matches, unique candidates, or entries needing attention.
5. Click a file’s **matching locations** dropdown to expand every matching source directly beneath the row. Each location shows its root and relative path; **Show in tree** reveals that entry in its folder. File details also show size, saved modification time, and comparison evidence. Import choices belong to the future merge plan.

```text
Copy 1
  ▾ Project
      ▾ Documents
          report.pdf
Copy 2
  ▾ Project backup
      report.pdf
      notes.txt
```

Every placement remains visible even when contents match. Partial samples screen candidates; only completed full hashes are shown as full-hash evidence. A match does not authorize deletion.

**Pause** saves a checkpoint. Resume the scan or comparison when ready. The app reopens the current saved job on launch; **Reopen saved job** reloads it from disk. **Re-scan** checks the saved sources again, refreshes the inventory, and invalidates old comparison evidence. Run **Compare contents** again for fresh evidence. **Recover / Resume** handles a saved job that needs SQLite recovery.

## Local files, local database

Folderloom bundles its interface inside a native desktop window. There is no hosted backend, external font download, or source-content upload.

SQLite is bundled with the app: using a packaged app requires no database server, Rust, Node, or Python installation. The app creates its data directory automatically. Each job uses its own **SQLite database**; files stay in their source folders. On macOS, the demo stores jobs under `~/Library/Application Support/com.folderloom.demo/jobs/`. Previous databases remain on disk when a new scan starts. A named-project history browser and cross-machine project portability are not implemented yet.

The demo loads the full inventory. Opening every folder or running a broad search can render every entry, so use small, bounded collections for now. Large-library responsiveness, direct NAS performance, and Windows support have not been validated. NAS folders must be mounted and accessible to the operating system before they can be selected.

## Build the desktop app

This is a development demo, not a signed or notarized downloadable release. Standalone packaging currently targets Apple silicon macOS. With Node 22.12+, npm, Rust, Python 3.9+, Rust's documentation component, and the macOS native build prerequisites installed:

```sh
cd apps/desktop
npm ci
npm run tauri dev
```

To build an application that opens without a development server:

```sh
npm run tauri build -- --bundles app
```

Open `target/release/bundle/macos/Folderloom Demo.app` from the repository root, or the equivalent path under your custom `CARGO_TARGET_DIR`. See [desktop development](docs/desktop-development.md) for private demo configuration and build details, and [engine and CLI development](docs/development.md) for command-line operations and core validation.

## Where this is going

The next step is reviewing suggested folder mappings and a saved merge plan around a chosen primary destination. Later steps will copy approved additions, verify destination contents and placements, then separately review source retention or recovery. Snapshot collections can remain separate where their structure matters.

The shared Rust engine drives both the desktop app and development CLI. Folderloom uses its own workflow and established libraries; it is not a Czkawka fork.

- [Product specification](docs/SPEC.md) and [tracking issue](https://github.com/mk11232/folderloom/issues/1)
- [Next milestone: folder mappings and saved plans](https://github.com/mk11232/folderloom/issues/5)
- [Architecture](docs/DESIGN.md), [desktop decision](docs/adr/0001-desktop-shell-and-reusable-rust-core.md), and [glossary](GLOSSARY.md)
- [Desktop validation](docs/desktop-validation.md) and [Czkawka evaluation](docs/czkawka-evaluation.md)

## Versions and contributions

Versions use CalVer **YYYY.M.release**: `2026.11.1`, `2026.11.2`, then `2026.12.1`. Each month's first published release starts at 1; months have no leading zero. This is a release identifier, not a promise of stable APIs or database compatibility. See [release practices](docs/releases.md).

Use synthetic development fixtures. Keep real files, client information, source inventories, job databases, and private screenshots outside the repository. Contributions must preserve the separation between review, copying, verification, and source cleanup.

## License and artwork

The current code and documentation are under the [MIT License](LICENSE). Dependencies retain their own terms. Interface icons use Lucide; the woven-folder application icon is custom AI-generated artwork. See [dependency and artwork notices](docs/licensing.md) for exact versions, bundled notices, source-availability links, and upstream qualifications.
