<img src="apps/desktop/src/assets/folderloom.png" alt="Three folders with matching document symbols flowing into a teal destination" width="112" />

# Folderloom

A desktop workspace for consolidating overlapping folder collections. Explore several folders, review where their files will go, then copy approved additions into a primary destination and verify the results.

**2026.11.2 · macOS development build · reviewed, copy-only consolidation**

The graphical app scans, compares, saves folder mappings and merge decisions, and copies an approved plan with destination verification. It keeps original source files in place. Source cleanup and recovery remain separate future work.

## Explore your folders

1. Open **Folderloom Demo.app** and choose **Add folder** for each collection you want to inspect. The **Primary destination** card lets you choose an existing folder or use one of the listed folders. Its existing contents stay in place.
2. Select the sources and choose **Scan selected folders**. Folderloom saves the inventory as it works.
3. Expand a folder to see its subfolders and files. Use **Expand all**, **Collapse all**, or search with **Command-F**. Search retains parent folders so every result has context.
4. Choose **Compare contents**. Filter for full-hash matches, unique candidates, or entries needing attention.
5. Click a file’s **matching locations** dropdown to expand every matching source directly beneath the row. Each location shows its root and relative path; **Show in tree** reveals that entry in its folder. File details also show size, saved modification time, and comparison evidence. Use the **Merge plan** step to choose incoming folders and exact destinations.

6. Open **Merge plan** and explicitly select the incoming source roots or subfolders that belong in the primary destination. Enter a relative target folder or leave it blank to combine at the primary.
7. Choose **Build merge plan** and review every exact destination. Matching contents at the same output path share one placement; differing contents get previewed conflict names. Exclude unwanted items before approving the displayed revision.
8. Choose **Copy approved additions**. **Results** distinguishes verified additions, verified existing files, folders, skipped items, and exceptions. Pause between files, reopen, and resume or retry unfinished work. **Open destination** opens its folder.

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

Every placement remains visible even when contents match. Partial samples screen candidates; only completed full hashes are shown as full-hash evidence. A match does not authorize deletion. Equal contents at different planned output paths retain both placements.

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
cargo fetch --locked
npm run tauri dev
```

To build an application that opens without a development server:

```sh
npm run tauri build -- --bundles app
```

Open `target/release/bundle/macos/Folderloom Demo.app` from the repository root, or the equivalent path under your custom `CARGO_TARGET_DIR`. See [desktop development](docs/desktop-development.md) for private demo configuration and build details, and [engine and CLI development](docs/development.md) for command-line operations and core validation.

## Where this is going

This build uses explicitly selected manual folder mappings. Automatic folder-matching suggestions, richer snapshot-preservation behavior, a historical job browser, and separately reviewed cleanup/recovery remain future work. Links and unsupported entries are held visibly. Directory metadata preservation is limited; inspect the per-item details. Regular-file metadata is copied on macOS, including extended attributes such as quarantine. Byte verification is not malware scanning. See the [consolidation workflow and limits](docs/consolidation-workflow.md).

The shared Rust engine drives both the desktop app and development CLI. Folderloom uses its own workflow and established libraries; it is not a Czkawka fork.

- [Product specification](docs/SPEC.md) and [tracking issue](https://github.com/mk11232/folderloom/issues/1)
- [Reviewed consolidation build](https://github.com/mk11232/folderloom/issues/9) and [broader folder-matching milestone](https://github.com/mk11232/folderloom/issues/5)
- [Architecture](docs/DESIGN.md), [desktop decision](docs/adr/0001-desktop-shell-and-reusable-rust-core.md), and [glossary](GLOSSARY.md)
- [Consolidation validation](docs/consolidation-validation.md), [earlier desktop validation](docs/desktop-validation.md), and [Czkawka evaluation](docs/czkawka-evaluation.md)

## Versions and contributions

Versions use CalVer **YYYY.M.release**: `2026.11.1`, `2026.11.2`, then `2026.12.1`. Each month's first published release starts at 1; months have no leading zero. This is a release identifier, not a promise of stable APIs or database compatibility. See [release practices](docs/releases.md).

Use synthetic development fixtures. Keep real files, client information, source inventories, job databases, and private screenshots outside the repository. Contributions must preserve the separation between review, copying, verification, and source cleanup.

## License and artwork

The current code and documentation are under the [MIT License](LICENSE). Dependencies retain their own terms. Interface icons use Lucide; the application icon is user-supplied artwork selected during the design process. See [dependency and artwork notices](docs/licensing.md) for exact versions, bundled notices, source-availability links, and upstream qualifications.
