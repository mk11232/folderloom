# Folderloom

Folderloom is a planned desktop application for consolidating overlapping folder collections into a chosen primary destination while preserving meaningful structure, differing file contents, and source history.

**Status: design and initial Rust scaffold.** The executable currently prints Hello World. Scanning, comparison, the desktop interface, copying, verification, and cleanup are not implemented yet.

## Intended workflow

1. Select multiple source roots and a primary destination.
2. Build a saved inventory with resumable progress and staged content comparison.
3. Review suggested folder mappings, new files, and conflicts in a merge plan.
4. Copy approved additions and verify their contents and destination placements.
5. Separately review whether to retain originals, move them to recovery, or remove verified source copies.

Preserve selected snapshots or dependent folder collections separately when their internal structure must remain intact. Identical contents at different meaningful paths retain their placements unless the user explicitly chooses otherwise.

## Design

- [Product specification](docs/SPEC.md)
- [Implementation issue](https://github.com/mk11232/folderloom/issues/1)
- [Architecture and delivery sequence](docs/DESIGN.md)
- [Requirements record](docs/requirements.md)
- [Domain glossary](GLOSSARY.md)
- [Desktop architecture decision](docs/adr/0001-desktop-shell-and-reusable-rust-core.md)
- [Czkawka evaluation](docs/czkawka-evaluation.md)

The accepted architecture is a reusable Rust engine, local SQLite job state, and a Tauri desktop interface using React and TypeScript. macOS with local and OS-connected NAS folders is the first target; Windows support will follow or join the initial release when validated.

Folderloom uses its own consolidation workflow and will build on established libraries and techniques from projects such as Czkawka. It is not a fork of Czkawka, and no upstream implementation has been copied into this scaffold.

## Run the current scaffold

Install a Rust toolchain and the platform's required build tools, then run:

```sh
cargo run
```

The first implementation milestone is a read-only inventory that persists, reopens, and resumes. See the specification for its acceptance criteria.

## Data and contributions

Use synthetic fixtures for development. Keep real files, client information, source inventories, job databases, and private handoff notes outside the public repository. Contributions should preserve the separation between review, copying, verification, and source cleanup.

## License

Folderloom's original code and documentation are licensed under the [MIT License](LICENSE). Third-party libraries and any future reused code retain their respective licenses and notices.
