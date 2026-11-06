# Folderloom

Folderloom consolidates overlapping folder collections while preserving unique files, differing contents, and their source locations.

## Language

**Consolidation job**:
A deliberately initiated effort to compare selected folder collections and combine their contents according to reviewed decisions.
_Avoid_: Sync, continuous synchronization

**Source root**:
A folder selected as a starting point for discovering folders and files to compare in a consolidation job. A job can include multiple source roots.
_Avoid_: Side A, side B

**Primary folder**:
The chosen destination and preferred folder structure for a consolidation. Choosing it does not make its file contents authoritative over differing contents found elsewhere.
_Avoid_: Winning copy, authoritative copy

**Primary root**:
The destination for a collection of consolidated folders, which may already contain folders or begin empty.
_Avoid_: Master backup

**Merge plan**:
A reviewable proposal identifying the selected source folders, destination paths, additions, and conflicts before any files are copied.
_Avoid_: Completed merge, sync

**Source history**:
The retained record of where a file came from and its relationship to a planned or completed addition.
_Avoid_: Filename tag

**Matched folder**:
An exact source folder confirmed to correspond to a destination folder. Its parent path is omitted from the destination mapping, while its descendants retain their relative structure unless a different mapping is approved.
_Avoid_: Matching tag, matching keyword

**File placement**:
A file's location within a folder structure, which can be meaningful even when identical contents exist at another location.
_Avoid_: Redundant file solely because its contents match

**Preserved snapshot**:
A source folder retained as a separate destination subtree with its internal structure, rather than merged into other folder collections.
_Avoid_: Combined collection

**Structure reference**:
A selected source whose folder layout provides the proposed starting structure for an empty primary destination. It does not determine which file versions are authoritative.
_Avoid_: Winning source

**Inventory**:
The saved record of discovered source folders and files, their recorded properties, and their examination progress within a consolidation job.
_Avoid_: Completed scan when examination is still pending
