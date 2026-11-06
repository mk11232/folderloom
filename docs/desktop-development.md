# Desktop development

## Build and run

The desktop interface uses the same public Rust `Job` operations as the CLI. It bundles its React interface inside the application; no hosted backend, external fonts, or source-content upload is involved. The demo exposes inventory and comparison only.

From `apps/desktop`, using Node 22.12+ (Node 26 was used during development), npm, Rust (including its documentation component), Python 3.9+, and the macOS native build prerequisites:

```sh
npm ci
npm run typecheck
npm run tauri dev
```

Build a standalone application with `npm run tauri build -- --bundles app`. From the repository root, open `target/release/bundle/macos/Folderloom Demo.app` in Finder (or the corresponding bundle under your custom `CARGO_TARGET_DIR`). `npm run build` checks TypeScript and produces the bundled frontend assets; `npm run dev` alone starts the frontend development server and cannot perform native job operations.

Local demo bundles use Tauri's [ad-hoc signing configuration](https://v2.tauri.app/distribute/sign/macos/#ad-hoc-signing), `bundle.macOS.signingIdentity: "-"`, so the build seals both the executable and application resources. Verify the finished bundle with `codesign --verify --deep --strict "target/release/bundle/macos/Folderloom Demo.app"` from the repository root. This is development signing, not Developer ID signing or Apple notarization; it does not establish approval for public distribution or remove macOS installation prompts.

The app reads an optional private `demo.json` from its application data directory (Tauri identifier `com.folderloom.demo`). Its shape is `{ "title": "Folder comparison", "description": "Local sample", "roots": [{ "label": "Copy 1", "path": "..." }] }`; supply actual local absolute paths privately, never commit them. When the configuration is absent, **Add folder** opens the native directory picker. Select sources and choose **Scan selected folders** to create a new saved job. Each new job retains the previous database on disk; the app automatically reopens the current saved job, and **Reopen saved job** reloads its saved state. This first interface does not include a historical job browser.

After inventory finishes, **Compare contents** runs size screening, edge samples, and full hashing through the core. **Pause** cooperatively stops after a saved checkpoint; **Resume scan** or **Resume comparison** continues the corresponding work. **Re-scan** starts a new validation generation for the current job and invalidates old comparison evidence. The source checkboxes apply to a new scan; re-scanning an existing job uses that job's saved roots.

Search paths with Command-F, filter by source or evidence, and select a placement to inspect the full root/relative path, recorded modification time, sample and full hashes, and other current full-hash placements. The expandable tree preserves folder context and lets each branch open or collapse independently. The current core read API loads the full inventory, and the tree has no pagination or rendering limit: expanding all folders or a broad search can display every entry. Keep this desktop demo to small, bounded inventories; large-inventory responsiveness has not been validated. Unique-size and unique-sample candidates remain visible. The gaps view includes inventory failures, root gaps, failed content reads, and their generations; stale entries can be inspected through **Needs attention**. Samples do not prove equality, and saved hashes do not authorize copying or cleanup.

Private source files, manifests, provenance, databases, and screenshots belong outside the public repository. Development fixtures must be synthetic. A developer's bounded local copy of a NAS sample is setup only; it does not establish native app support for scanning a live NAS or for Windows. Packaged macOS interaction and persistence validation are recorded separately from frontend type/build checks.


Standalone packaging currently requires Apple silicon macOS because that is the reviewed dependency-notice target. Other targets fail before packaging until their notices are reviewed. See [licensing](licensing.md).
