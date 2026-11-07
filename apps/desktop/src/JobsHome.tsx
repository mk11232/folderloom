import { createPortal } from "react-dom";
import type { ReactNode } from "react";
import { useEffect, useId, useRef, useState } from "react";
import { Archive, ArchiveRestore, FolderOpen, LoaderCircle, Plus, RefreshCw, Trash2 } from "lucide-react";
import type { JobSummary } from "./api";
import { jobStatus } from "./job-workflow";

type Props = { jobs: JobSummary[]; loading: boolean; busy: boolean; workingJob: string | null; error: string | null; hasDraft: boolean; onNew: () => void; onDraft: () => void; onOpen: (id: string) => void; onRefresh: () => void; onArchive: (id: string, archived: boolean) => void; onDelete: (id: string) => Promise<void>; onDiscardDraft: () => void };
export function JobsHome({ jobs, loading, busy, workingJob, error, hasDraft, onNew, onDraft, onOpen, onRefresh, onArchive, onDelete, onDiscardDraft }: Props) {
  const [showArchived, setShowArchived] = useState(false);
  const [deleting, setDeleting] = useState<JobSummary | null>(null);
  const visibleJobs = jobs.filter(job => Boolean(job.archived) === showArchived);
  return <main className="jobs-home"><header className="home-heading"><div><span className="eyebrow">YOUR LOCAL WORKSPACE</span><h1>Jobs</h1><p>Open a saved scan, continue a consolidation, or start with a fresh set of folders.</p></div><div><button disabled={loading || busy} onClick={onRefresh}><RefreshCw size={14} /> Refresh</button><button className="primary" disabled={loading || busy || Boolean(workingJob)} onClick={onNew}><Plus size={15} /> New job</button></div></header>
    {error && <div className="alert error" role="alert"><p>{error}</p><button onClick={onRefresh} disabled={loading}>Retry</button></div>}
    {workingJob && <div className="working-banner"><LoaderCircle size={15} className="spin" /><span>A job is working. You can return to it now; other jobs can open once it pauses or finishes.</span><button disabled={busy} onClick={() => onOpen(workingJob)}>Return to working job</button></div>}
    {hasDraft && <div className="draft-banner"><div><strong>Unfinished job setup</strong><p>Your source choices and primary destination are saved locally.</p></div><button disabled={loading || busy || Boolean(workingJob)} onClick={onDraft}>Continue setup</button><button disabled={loading || busy || Boolean(workingJob)} onClick={onDiscardDraft}>Discard setup</button></div>}
    <div className="jobs-filter"><button aria-pressed={!showArchived} onClick={() => setShowArchived(false)}>Current ({jobs.filter(job => !job.archived).length})</button><button aria-pressed={showArchived} onClick={() => setShowArchived(true)}>Archived ({jobs.filter(job => job.archived).length})</button><p>Archiving hides a job from Current. Its history and files stay intact.</p></div>
    <section className="jobs-list" aria-label="Saved jobs"><table aria-label="Saved jobs"><thead><tr><th>Job</th><th>Status / stage</th><th>Sources</th><th>Entries</th><th>Primary destination</th><th>Updated</th><th /></tr></thead><tbody>{visibleJobs.map(job => <tr key={job.id}><td><strong>{job.name}</strong>{job.error && <small className="job-error">{job.error}</small>}</td><td><span className={job.active ? "job-active" : ""}>{jobStatus(job)}</span><small>{job.stage === "inventory" ? "Scan & compare" : job.stage === "plan" ? "Merge plan" : "Results"}</small></td><td>{job.source_count.toLocaleString()}</td><td>{job.entry_count.toLocaleString()}</td><td className="job-destination">{job.destination || "Not selected"}</td><td>{new Date(job.updated_ms).toLocaleString()}</td><td><div className="job-actions"><button className="job-open" aria-label={`Open job ${job.name}`} title="Open job and resume its saved stage" disabled={busy || Boolean(workingJob && workingJob !== job.id)} onClick={() => onOpen(job.id)}><FolderOpen size={14} /> Open job</button><JobActionButton label={`${job.archived ? "Restore" : "Archive"} ${job.name}`} explanation={job.archived ? "Restore job — move it back to Current." : "Archive job — hide it from Current. You can restore it later; its saved history stays here."} disabled={loading || busy || Boolean(workingJob)} onClick={() => onArchive(job.id, !job.archived)}>{job.archived ? <ArchiveRestore size={16} /> : <Archive size={16} />}</JobActionButton>{job.archived && <JobActionButton danger label={`Delete ${job.name} permanently`} explanation="Delete permanently — removes saved job history. The deleted job cannot be recovered. Source folders and copied files stay untouched." disabled={loading || busy || Boolean(workingJob)} onClick={() => setDeleting(job)}><Trash2 size={16} /></JobActionButton>}</div></td></tr>)}</tbody></table>{!visibleJobs.length && <div className="home-empty">{loading ? <LoaderCircle className="spin" size={25} /> : <FolderOpen size={28} />}<h2>{loading ? "Opening saved jobs…" : showArchived ? "No archived jobs" : jobs.length ? "No current jobs" : "Your first job starts here"}</h2><p>{loading ? "Reading local job records." : showArchived ? "Archived jobs appear here. Restore one whenever you need it." : "Choose source folders and a primary destination. Each job keeps its own inventory, plan, and results."}</p>{!loading && !showArchived && <button className="primary" disabled={loading || busy || Boolean(workingJob)} onClick={onNew}>New job</button>}</div>}</section>
    {deleting && <DeleteJobDialog job={deleting} busy={busy || Boolean(workingJob)} onCancel={() => setDeleting(null)} onDelete={onDelete} />}
  </main>;
}

function DeleteJobDialog({ job, busy, onCancel, onDelete }: { job: JobSummary; busy: boolean; onCancel: () => void; onDelete: (id: string) => Promise<void> }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [failure, setFailure] = useState<string | null>(null);
  useEffect(() => { dialog.current?.showModal(); }, []);
  async function remove() {
    setFailure(null);
    try { await onDelete(job.id); onCancel(); }
    catch (error) { setFailure(error instanceof Error ? error.message : String(error)); }
  }
  return <dialog ref={dialog} className="delete-job-dialog" aria-labelledby="delete-job-title" aria-describedby="delete-job-description" onCancel={event => { event.preventDefault(); if (!busy) onCancel(); }}>
    <div className="delete-job-symbol"><Trash2 size={22} /></div>
    <h2 id="delete-job-title">Delete “{job.name}” permanently?</h2>
    <p id="delete-job-description">This removes this job’s saved scans, comparisons, merge plans, and results from this computer. The deleted job cannot be recovered.</p>
    <p className="files-retained">Your source folders and copied files stay untouched.</p>
    {failure && <p role="alert" className="inline-error">{failure}</p>}
    <div className="delete-job-actions"><button autoFocus disabled={busy} onClick={onCancel}>Keep job</button><button className="delete-confirm" disabled={busy} onClick={() => void remove()}>{busy ? "Deleting…" : "Delete permanently"}</button></div>
  </dialog>;
}

function JobActionButton({ label, explanation, danger = false, disabled, onClick, children }: { label: string; explanation: string; danger?: boolean; disabled: boolean; onClick: () => void; children: ReactNode }) {
  const button = useRef<HTMLButtonElement>(null);
  const tooltipId = useId();
  const [position, setPosition] = useState<{ left: number; top: number } | null>(null);
  function show() {
    const rect = button.current?.getBoundingClientRect();
    if (!rect) return;
    const width = 260;
    setPosition({ left: Math.max(12, Math.min(rect.right - width, window.innerWidth - width - 12)), top: rect.bottom + 110 < window.innerHeight ? rect.bottom + 8 : Math.max(12, rect.top - 108) });
  }
  useEffect(() => {
    if (!position) return;
    const hide = () => setPosition(null);
    window.addEventListener("scroll", hide, true);
    window.addEventListener("resize", hide);
    return () => { window.removeEventListener("scroll", hide, true); window.removeEventListener("resize", hide); };
  }, [position]);
  return <><button ref={button} className={`job-icon-action${danger ? " job-delete" : ""}`} aria-label={label} aria-describedby={position ? tooltipId : undefined} disabled={disabled} onMouseEnter={show} onMouseLeave={() => setPosition(null)} onFocus={show} onBlur={() => setPosition(null)} onKeyDown={event => { if (event.key === "Escape") setPosition(null); }} onClick={() => { setPosition(null); onClick(); }}>{children}</button>{position && createPortal(<div id={tooltipId} role="tooltip" className={`job-action-tooltip${danger ? " destructive-tooltip" : ""}`} style={position}>{explanation}</div>, document.body)}</>;
}
