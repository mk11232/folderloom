# Czkawka reuse and hashing evaluation

Status: Historical read-only source review. The user accepted building Folderloom's own workflow with established libraries and ideas from Czkawka rather than forking the whole application. BLAKE3 is now integrated into Folderloom's staged comparison foundation. No upstream implementation has been copied and no performance benchmark has been run.

Reviewed upstream commit: `7b2383f88baf73aac99c99992fe54321b63b7f6a` in [qarmin/czkawka](https://github.com/qarmin/czkawka/tree/7b2383f88baf73aac99c99992fe54321b63b7f6a). The user identified Czkawka as the earlier example. No NAS scan, upstream build, or timing comparison was performed.

## Observed behavior

- Duplicate discovery groups files by size and removes singleton size groups from its duplicate pipeline: [core.rs, lines 204-278](https://github.com/qarmin/czkawka/blob/7b2383f88baf73aac99c99992fe54321b63b7f6a/czkawka_core/src/tools/duplicate/core.rs#L204-L278).
- Prehash reads the first and last 4 KiB, or the whole small file when those regions overlap: [mod.rs, constant and implementation](https://github.com/qarmin/czkawka/blob/7b2383f88baf73aac99c99992fe54321b63b7f6a/czkawka_core/src/tools/duplicate/mod.rs#L222-L275).
- Remaining candidate groups receive full hashes. Cache saves happen after collecting hashing results; a cooperative stop can save collected work, but this is not evidence of a per-file durable checkpoint under abrupt process termination: [core.rs, hashing and cache saves](https://github.com/qarmin/czkawka/blob/7b2383f88baf73aac99c99992fe54321b63b7f6a/czkawka_core/src/tools/duplicate/core.rs#L375-L625).
- Cache matching uses path, size, and modification date: [cache.rs](https://github.com/qarmin/czkawka/blob/7b2383f88baf73aac99c99992fe54321b63b7f6a/czkawka_core/src/common/cache.rs#L146-L198). These are useful cache hints, not fresh full-content verification for cleanup.
- The core is separate from its GUIs and carries an MIT license. It also has broad unconditional dependencies for image, audio, archive, and document tools: [Cargo.toml](https://github.com/qarmin/czkawka/blob/7b2383f88baf73aac99c99992fe54321b63b7f6a/czkawka_core/Cargo.toml), [core license](https://github.com/qarmin/czkawka/blob/7b2383f88baf73aac99c99992fe54321b63b7f6a/czkawka_core/LICENSE_MIT). Other repository components have different license files; any actual reuse needs attribution and a check of the exact copied components and dependencies.

## Recommendation

Keep Folderloom's application and durable inventory independent rather than forking the whole Czkawka application. Reuse established hashing libraries and the staged comparison strategy. Evaluate direct core-library reuse against selective adaptation before importing it; the existing duplicate-result pipeline alone cannot supply Folderloom's complete inventory, unique-file coverage, placement mapping, review decisions, or recovery journal.

Proposed pipeline:

1. Persist every discovered entry and directory-enumeration status, including unique files, errors, excluded entries, and relevant empty folders.
2. Use size to eliminate impossible exact-content matches from duplicate comparison, without removing them from inventory.
3. Sample large candidates with small head/tail reads. A differing sample establishes a difference at the time read; matching samples remain candidates only. Hash small files once in full.
4. Use the maintained [BLAKE3 Rust implementation](https://github.com/BLAKE3-team/BLAKE3) for full-content fingerprints. Store algorithm and sample-version metadata so partial and full fingerprints cannot be confused.
5. Persist completed per-file work in bounded transactions. After interruption, revalidate completed records and repeat only unfinished or stale work; interrupted hashing is never marked complete.
6. Hash unique additions while copying when appropriate, and independently verify the destination. Fresh safety checks for cleanup remain necessary even when discovery used cached hashes.

The fastest settings are workload-dependent. On a NAS, network latency, storage throughput, and access patterns can dominate hash CPU time. Benchmark sample size, small-file cutoff, and bounded concurrent reads on synthetic or explicitly selected data before claiming a fastest configuration. Partial hashes do not avoid full reads when the candidates genuinely match.

## Validation before reuse

Compare a narrow integration of czkawka_core with a small Folderloom inventory pipeline using the same BLAKE3 library. Measure bytes read, wall time, peak memory, dependency/build cost, cooperative pause, abrupt termination, and stale-cache behavior. Test same-size/same-prefix-but-different-middle files, changed sources, unique placements, disconnected shares, and resume. Do not run upstream deletion actions as part of this evaluation.
