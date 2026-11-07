# Reviewed consolidation

This workflow extends inventory and comparison into a saved, copy-only merge plan. Sources remain in place. Source cleanup, recovery moves, and permanent removal are not part of this build.

## Review flow

1. Start at Jobs and open a saved job, or choose New job. Name the job and add/remove local or mounted source folder selections. Then choose the primary destination and start the scan. Existing comparison and inline matching-location views remain available.
2. Use the always-visible Primary destination card to choose an existing directory (empty or populated), or select one of the listed source folders as primary. Select the source folders or subfolders to contribute. The destination cannot overlap an incoming selected folder. A scanned primary can be left unselected as an input.
3. Map each selected folder to a relative folder inside the primary. An empty target means the primary itself. The selected source folder defines the boundary: wrapper folders above it are omitted. Use a separate target folder to preserve a collection's internal layout.
4. Build the saved merge plan. Review exact output paths, matched contributions, already-present files, additions, generated conflict names, and blocked entries. Exclude individual plan items or edit mappings and rebuild. Every changed plan revokes prior approval.
5. Approve the displayed revision, then copy approved additions. Copies use fresh content checks, staging, no-clobber publication, and destination verification. Pause takes effect between files; a large file may take time to finish. Reopening preserves decisions and operation results.
6. Inspect results and open the destination. Failed or changed items remain visible. Resume or retry unfinished work after resolving the reported problem; changed plans require another review.

Identical contents at the same intended output path require one destination placement, with all contributions recorded. Identical contents at different intended paths retain both placements. Existing destination files are never overwritten; differing versions receive previewed conflict names. A partial or unsupported result must not imply that the source can be deleted.

## Current boundaries

Planning is a fresh, read-only examination and saves a complete plan atomically. It currently has no mid-plan pause/checkpoint. Execution pauses between individual files and durably records each result. Retrying revalidates completed placements as well as unfinished items; it does not blindly trust a previous successful copy.

Mapping suggestions are manual in this build. Separate snapshot targets should use a new empty destination subfolder; there is not yet a dedicated immutable-snapshot mode. Empty folders are included by the default inventory configuration. Directory contents and placement are preserved, but directory timestamps, ACLs and other directory metadata are explicitly reported as not copied. Held links and unsupported objects remain visible and untouched.

No-clobber file publication requires filesystem support for the chosen atomic publication operation. An unsupported filesystem operation is reported as a failed item, never replaced by an overwrite. Local macOS validation does not establish compatibility with every mounted NAS filesystem. Windows is not implemented by the current Unix core.

## Malware scanning decision

Malware scanning is a possible optional integration, not the purpose of the consolidation engine and not included in this build. Content verification means copied bytes match the reviewed source; it does not mean a file is safe to open. Folderloom does not execute copied files or automatically open them. Opening the destination opens its folder.

A future integration can invoke a separately installed scanner with structured arguments, record scanner/definition versions and file observations, and distinguish not scanned, no threats detected, flagged, and scan failed. Flagged files should be held for review; scanner integration must not automatically delete originals or upload private contents. Changes after scanning invalidate that result. No-threat findings are not guarantees.

ClamAV supports standalone `clamscan` without a running daemon; `clamdscan` uses a separately running service. Signature updates, installation, performance, licensing, and result parsing need a separate integration review. macOS XProtect is a platform protection layer, not evidence that Folderloom has scanned every selected source file.

Sources: [ClamAV scanning documentation](https://docs.clamav.net/manual/Usage/Scanning.html), [Apple platform malware protection](https://support.apple.com/guide/security/protecting-against-malware-sec469d47bd8/web).
