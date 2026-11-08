# Native quit lifecycle

Implemented locally on 2026-11-07. This is the first interruption milestone; it does not add within-file copy cancellation or a duplicate-search mode.

## Behavior

Native window close and application Quit share a backend request. With an active worker, the window stays open and offers **Pause and quit** or **Keep working**. Repeated requests keep the same confirmation. Accepting requests the existing cooperative pause; the app exits only after the worker returns, drops its Job, and publishes its stopped/error outcome under the snapshot mutex.

The confirmation remains open if the operation finishes while the user is deciding. A worker error keeps the app open for review instead of claiming a successful saved pause. An explicit later quit is allowed after dismissing the failed request. If internal mutex state is poisoned, native quit is allowed as an interrupted exit rather than trapping the user; this is not reported as a successful pause.

During the wait, **Keep app open** withdraws the quit request but does not retract a pause that may already be saving. Each confirmation has a request number, so stale confirmations, cancellations, and exit acknowledgements cannot affect a later request. Cancellation received before the final exit reservation prevents that exit; cancellation cannot undo an exit already committed. The quit gate also prevents a new worker from starting during confirmation, waiting, or exit.

The root-level dialog polls a small control payload independently of the inventory snapshot. It uses a modal HTML dialog with a title, description, keyboard focus trapping, Escape cancellation, and initial focus on Keep working. Exit acknowledgement depends on the renderer remaining responsive; a stalled renderer or blocked filesystem call can delay cooperative exit. Force quit and OS termination remain interruption/recovery cases.

Current boundaries: scan batches; byte-bounded comparison steps; whole copied files; completed atomic plans. No promise of immediate cancellation during a huge copy or a stalled NAS call. No new crash/publication reconciliation logic was added.

## Validation

- Desktop tests: 32 passed across seven new lifecycle tests and the existing 25 desktop integration tests. The full adapter suite passed before the final poison fallback; all seven affected lifecycle tests passed again after that change.
- Lifecycle cases: every worker mode waits for release; new-work admission is rejected during quit; repeated requests remain one request; Keep working does not initiate a pause; cancellation defeats a late acknowledgement; old request IDs cannot affect a later dialog; worker failure prevents automatic exit; poisoned snapshot/quit locks do not trap native quit; a real synthetic scan saves, reopens, and resumes to completion.
- Frontend: 29 existing tests passed; TypeScript and Vite production build passed.
- `cargo clippy -p folderloom-desktop --all-targets --all-features -- -D warnings` passed after addressing its match-style finding.
- Native bundle build and installed-app strict code-signature verification passed. SDK 26.5 was selected explicitly; the initial default SDK 27 test link failed on unsupported `arm64e.x1-macos` stubs, before tests ran.
- Claude Fable 5.1 reviewed the concrete backend/UI patch (bridge job `0a75af74-0fba-4d9b-a9f3-5ae5cffcc911`). Its blocker about poisoned-state quit handling was addressed with a tested native-exit fallback. The review was text-only; it did not run the app or tests.

## Native observations

The installed demo was exercised with a disposable 12,000-file synthetic collection. Window close during rescan showed a single accessible confirmation; a repeated close preserved it. Keep working dismissed it and retained the workspace. A saved scan reopened and resumed to 12,001 entries including its root.

The same disposable job was refreshed against four sparse synthetic 8 GiB files. Window close during content comparison showed the confirmation; choosing Pause and quit exited; reopening showed **Comparison paused — resume**. This proves the native close/dialog/checkpoint/reopen path for comparison, not NAS throughput, within-file copying, or a storage benchmark. Keyboard Quit was also exercised, but intermediate shortcut timing was not reliably observed through automation; the native close path supplies the explicit visual confirmation evidence. Native copy/planning waits and cancellation of a long pending wait were covered by deterministic lifecycle tests, not full native filesystem runs.

Folder-chooser automation was unreliable while preparing the test; its destination was seeded into this test-only draft while the app was closed. The test job, its files, and its database sidecars were removed afterward; the previous selected job was restored. The final app was reopened showing the original two current jobs and one archived job, without a test draft. No user source/output files were changed.

Installed: `~/Applications/Folderloom Demo.app`. Local development build only; no push, merge, notarization, or release.
