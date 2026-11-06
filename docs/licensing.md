# Dependency and artwork notices

Folderloom's original code is MIT licensed. Dependencies retain their own licenses; Folderloom's license does not replace them. The checked-in distribution notices are in [`apps/desktop/src-tauri/resources/licenses`](../apps/desktop/src-tauri/resources/licenses). Tauri bundles this directory as `Contents/Resources/licenses` in the macOS application. In Finder, choose **Show Package Contents** to read the notices, dependency inventory, and Rust standard-library notices.

## Current inventory

The generated `inventory.json` records the exact lockfile hashes, package versions, upstream source archive locations, license declarations, and references to verbatim license texts. This review targets the `aarch64-apple-darwin` release with the desktop and custom-protocol features. It conservatively includes the Rust production dependency graph and its build dependencies, excluding workspace test dependencies. It is not a symbol-by-symbol binary attribution report. Future targets need their own dependency review.

There are 255 external Rust packages, six production npm packages, SQLite bundled through `libsqlite3-sys`, Feather attribution, and a referenced MIT license text. The Rust toolchain's exact `COPYRIGHT-library.html` and supplied license texts cover the standard library and its dependencies separately. That upstream collection is broader than the code necessarily linked into this app; inclusion of a notice is not a claim that every covered component ships in the executable. npm-only development tools are not bundled in the frontend and are excluded.

The frontend dependencies locked for this demo are:

| Package | Version | Declared license |
| --- | --- | --- |
| `lucide-react` | 0.468.0 | ISC, with upstream Feather/MIT attribution |
| `react` | 19.3.1 | MIT |
| `react-dom` | 19.3.1 | MIT |
| `scheduler` | 0.28.0 | MIT |
| `@tauri-apps/api` | 2.12.4 | Apache-2.0 OR MIT |
| `@tauri-apps/plugin-dialog` | 2.8.4 | MIT OR Apache-2.0 |

Lucide supplies the interface glyphs. Its installed license explicitly credits the Feather-derived portions; the distribution includes that exact ISC notice plus the upstream Feather MIT notice. The generated inventory includes all npm production packages even when bundler tree-shaking removes unused exports.

## Qualifications and source availability

Five Rust packages declare MPL-2.0: `cssparser` 0.37.0, `cssparser-macros` 0.7.1, `dtoa-short` 0.3.5, `option-ext` 0.2.0, and `selectors` 0.38.0. Their unchanged source archives are linked by exact version in the shipped inventory and notices. Their source remains available under MPL-2.0, and Folderloom adds no restrictions to those source rights. Mozilla's [distribution guidance](https://www.mozilla.org/en-US/MPL/2.0/FAQ/#using-and-distributing-software-under-the-license) explains the source-availability notice obligation for executables built from unchanged MPL code. If these dependencies are modified, distribute the modified covered source and update the notice accordingly.

The exact-release [`objc2` upstream notice](https://github.com/madsmtm/objc2/blob/d7d2fa23ceaa5e6096c923b081040e5d81b3b9df/LICENSE.md) declares MIT or Zlib/Apache-2.0/MIT licenses, depending on the crate, but also expressly questions the redistribution status of bindings derived from Apple SDKs under Xcode's terms. The complete upstream qualification is preserved in the shipped notices. This review does not resolve that upstream uncertainty or provide blanket clearance of Apple SDK-derived material. System frameworks and the installed WebKit runtime are supplied by macOS, not copied into this repository as dependencies.

Some published crates omit standalone license files. Their supplemental notices are taken from the exact commit in their `.cargo_vcs_info.json`, with source URL and SHA-256 recorded in [`third-party/supplemental.json`](../third-party/supplemental.json). `selectors` links the normative MPL text from its source header; its supplement is that text. The objc2 notices link the canonical MIT terms rather than carrying a standalone copyright statement; the supplement retains that upstream document and a clearly identified canonical license reference without inventing a copyright owner. SQLite's public-domain dedication is extracted from the bundled amalgamation itself.

## Custom icon and fonts

The woven-folder application icon is custom AI-generated artwork made for Folderloom from the requested concept: loose strands woven through a loom into a completed folder. It is distinct from the Lucide interface glyphs. The source image and PNG/ICNS derivatives are recorded in [`apps/desktop/src/assets/README.md`](../apps/desktop/src/assets/README.md). No third-party reference image or private source file was supplied for that generation. This provenance record does not guarantee exclusive rights, copyright protection, or absence of similarity to other artwork.

The interface uses CSS system-font fallbacks. No font files are redistributed. Font availability and rendering depend on the user's operating system.

## Regeneration and release check

After `npm ci` in `apps/desktop` and fetching the locked Cargo dependencies, run from the repository root:

```sh
python3 scripts/third_party_notices.py
python3 scripts/third_party_notices.py --check
```

Python 3.9+, Cargo, Rust, and the installed Rust documentation component are required. The generator runs offline, traverses the locked desktop dependency graph, checks installed npm versions against the lockfile, validates hashes of vendored upstream supplements, and fails on missing declared licenses or uncovered packages. It does not substitute a generic license for missing attribution. Output has no timestamps or local filesystem paths. Repeating generation against the same dependencies and Rust toolchain produces identical bytes. A toolchain change regenerates its standard-library notices.

Tauri's production build runs the generator before bundling. Review changes to `inventory.json`, new license expressions, supplemental source provenance, and upstream qualifications whenever dependencies change. A generated inventory and preserved notices establish what was reviewed; they are not a claim that every possible licensing or intellectual-property issue has been resolved.
