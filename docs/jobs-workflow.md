# Saved jobs and guided setup

Folderloom opens to a local Jobs table rather than reopening a single workspace. A job row identifies the saved scan or consolidation, its sources, destination, stage and last update. Opening it reloads its own inventory, plan, decisions and results. No account or hosted backend is required.

## New job

Sources → Primary destination → Scan and compare → Merge plan → Results.

Before scanning, the setup is a durable draft. Add and remove source selections, name the job, then choose an existing primary folder. Removing a source does not delete its directory or contents. Selecting a listed source as the primary removes it from incoming discovery. Existing primary contents are compared during plan construction and never overwritten.

Starting the scan creates a distinct saved job. Its source set is historical evidence. To revise that set, use the job as a starting point for a new draft; this creates a new scan rather than changing the old plan's source identities. Changing mappings or destination on a saved job still requires rebuilding and approving a new plan revision.

The job stages distinguish inventory completion from copy completion. A scan that finished is not a completed consolidation. Reopening interrupted work leaves it stopped until the user resumes it. One background filesystem worker runs at a time; starting or switching jobs is blocked while it is working. Returning to Jobs can show the running job without interrupting it.

## Storage

Inventory, comparisons, decisions and copy checkpoints remain in each job's bundled SQLite database. The desktop host stores setup metadata and a draft in small local files, using atomic replacement. Historical jobs created before the wizard are discovered from existing databases and remain accessible. An unreadable job remains visible with an error rather than silently disappearing.

This adds navigation and setup to the existing [copy-only workflow](consolidation-workflow.md). It does not implement job deletion, source cleanup, automatic folder matching, cross-computer project transfer or malware scanning.

## Archive and restore

The Jobs table separates Current and Archived jobs. Archive and Restore persist locally across app restarts and keep each job database, setup, sources, and destination intact. Archived jobs can still be opened for review. Archive and Restore are unavailable while a filesystem operation is running. Older jobs without archive state remain Current.

Archived jobs also offer Delete permanently. The confirmation names the job and explains the loss of saved scans, comparisons, plans, and results. Deletion removes only that job’s local SQLite database, SQLite sidecars, setup metadata, and archive marker. Source and destination files, other jobs, and the setup draft are retained. A running operation blocks deletion; deleting the selected job clears the saved selection.
