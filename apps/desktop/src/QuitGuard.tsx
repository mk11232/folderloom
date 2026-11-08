import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { api, type QuitStatus } from "./api";

// This small control poll stays independent of the potentially large job snapshot.
export default function QuitGuard() {
  const [status, setStatus] = useState<QuitStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const version = useRef(0);
  const acting = useRef(false);

  useEffect(() => {
    if (!isTauri()) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      const current = version.current;
      try {
        if (!acting.current) {
          const next = await api.quitStatus();
          if (!stopped && current === version.current) {
            setStatus(next);
            if (next.phase === "waiting" && !next.worker) {
              await api.finishQuit(next.request);
            }
          }
        }
      } catch (failure) {
        if (!stopped && current === version.current) setError(String(failure));
      } finally {
        if (!stopped) timer = setTimeout(() => void poll(), 250);
      }
    }
    void poll();
    return () => { stopped = true; clearTimeout(timer); };
  }, []);

  const open = status !== null && !["idle", "exiting"].includes(status.phase);
  useEffect(() => {
    if (open && !dialog.current?.open) dialog.current?.showModal();
    if (!open && dialog.current?.open) dialog.current.close();
  }, [open]);

  async function act(confirm: boolean) {
    if (!status || acting.current) return;
    acting.current = true;
    version.current += 1;
    setBusy(true);
    setError(null);
    try {
      setStatus(await (confirm ? api.confirmQuit(status.request) : api.cancelQuit(status.request)));
    } catch (failure) { setError(String(failure)); }
    finally { acting.current = false; setBusy(false); }
  }

  const waiting = status?.phase === "waiting";
  const failed = status?.phase === "failed";
  const boundary = status?.worker === "execute"
    ? "Copying pauses after the current file finishes. A large file or slow drive may take a while."
    : status?.worker === "plan"
      ? "The current merge plan must finish saving before Folderloom can quit."
      : "Folderloom will stop at the next saved checkpoint. A slow or disconnected drive may delay this.";

  return <dialog ref={dialog} className="quit-dialog" aria-labelledby="quit-title" aria-describedby="quit-description"
    onCancel={event => { event.preventDefault(); void act(false); }}>
    <h2 id="quit-title">{failed ? "Couldn’t finish pausing" : waiting ? "Saving progress before quitting…" : "Pause this job and quit?"}</h2>
    <p id="quit-description">{failed
      ? "Folderloom is staying open because the operation reported a problem. Review the job before quitting again."
      : waiting ? `${boundary} You can keep the app open; the requested pause will still finish.`
      : `${boundary} Your saved job will be available when you reopen the app.`}</p>
    {waiting && <p className="quit-waiting" role="status">Waiting for the worker to stop and save.</p>}
    {(error || (failed && status?.error)) && <p className="alert error" role="alert">{error || status?.error}</p>}
    <div className="quit-actions">
      <button autoFocus disabled={busy} onClick={() => void act(false)}>{waiting || failed ? "Keep app open" : "Keep working"}</button>
      {!waiting && !failed && <button className="primary" disabled={busy} onClick={() => void act(true)}>Pause and quit</button>}
    </div>
  </dialog>;
}
