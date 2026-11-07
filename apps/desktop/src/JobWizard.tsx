import { useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowLeft, ArrowRight, Folder, FolderPlus, LoaderCircle, Play, X } from "lucide-react";
import { api } from "./api";
import type { JobSetup } from "./api";
import { createDraftWriter, incomingSources, pathsOverlap, setupProblem } from "./job-workflow";

type Props = { initial: JobSetup; busy: boolean; onCancel: () => void; onStart: (setup: JobSetup) => Promise<boolean> };
export function JobWizard({ initial, busy, onCancel, onStart }: Props) {
  const [setup, setSetup] = useState<JobSetup>(initial);
  const [stage, setStage] = useState<"sources" | "destination">("sources");
  const [saving, setSaving] = useState(false);
  const [choosing, setChoosing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const writer = useRef(createDraftWriter(api.saveDraft));
  const version = useRef(0);
  const locked = busy || choosing;
  async function persist(next: JobSetup) {
    setSetup(next); setSaving(true); setError(null);
    const current = ++version.current;
    try { await writer.current.save(next); }
    catch (failure) { if (current === version.current) setError(`Could not save setup: ${String(failure)}`); }
    finally { if (current === version.current) setSaving(false); }
  }
  async function leave() {
    try { await writer.current.flush(); onCancel(); }
    catch { setError("Save this setup successfully before returning to Jobs. Retry saving below."); }
  }
  async function pick(kind: "source" | "destination") {
    setChoosing(true); setError(null);
    try {
      const paths = await open({ directory: true, multiple: kind === "source", title: kind === "source" ? "Add source folders to this job" : "Choose an existing primary destination" });
      if (kind === "destination" && typeof paths === "string") await persist({ ...setup, destination: paths });
      if (kind === "source" && paths) {
        const additions = (Array.isArray(paths) ? paths : [paths]).filter(path => !setup.sources.some(source => source.path === path));
        await persist({ ...setup, sources: [...setup.sources, ...additions.map(path => ({ path, label: path.replace(/\/+$/, "").split("/").at(-1) || path }))] });
      }
    } catch (failure) { setError(String(failure)); }
    finally { setChoosing(false); }
  }
  async function next() {
    try { await writer.current.flush(); setStage("destination"); }
    catch { setError("Save the source choices before continuing. Retry saving below."); }
  }
  async function start() {
    const problem = setupProblem(setup);
    if (problem) { setError(problem); return; }
    try { await writer.current.flush(); await onStart(setup); }
    catch (failure) { setError(String(failure)); }
  }
  return <main className="setup-wizard"><header className="home-heading"><div><span className="eyebrow">NEW JOB</span><h1>{stage === "sources" ? "Choose your source folders" : "Choose your primary destination"}</h1><p>{stage === "sources" ? "This setup belongs to one job. Add or remove folders before scanning." : "The primary can be an existing collection or an empty folder. Its contents remain in place."}</p></div><button disabled={locked || saving} onClick={() => void leave()}>Back to Jobs</button></header>
    <nav className="wizard-track" aria-label="Job setup progress"><span aria-current={stage === "sources" ? "step" : undefined}>1 Sources</span><ArrowRight size={14} /><span aria-current={stage === "destination" ? "step" : undefined}>2 Destination</span><ArrowRight size={14} /><span>3 Scan & compare</span><ArrowRight size={14} /><span>4 Merge plan</span><ArrowRight size={14} /><span>5 Results</span></nav>
    {error && <div className="alert error" role="alert"><p>{error}</p><button disabled={locked || saving} onClick={() => void persist(setup)}>Retry saving</button></div>}
    <section className="setup-card">{stage === "sources" ? <><label className="job-name-field"><span>Job name</span><input aria-label="Job name" placeholder="Family archive · November" value={setup.name} disabled={locked} onChange={event => void persist({ ...setup, name: event.target.value })} /></label><div className="section-heading"><div><h2>Source folders <span>{setup.sources.length}</span></h2><p>Remove changes this selection only. It never deletes a folder or its files.</p></div><button disabled={locked} onClick={() => void pick("source")}><FolderPlus size={15} /> Add folders</button></div><div className="setup-sources">{setup.sources.map(source => <div className="setup-source" key={source.path}><Folder size={18} /><div><strong>{source.label}</strong><small>{source.path}</small>{pathsOverlap(source.path, setup.destination) && <em>Primary area · review on Destination step</em>}</div><button aria-label={`Remove source ${source.label}`} disabled={locked} onClick={() => void persist({ ...setup, sources: setup.sources.filter(candidate => candidate.path !== source.path) })}><X size={14} /> Remove</button></div>)}{!setup.sources.length && <p className="plan-empty">Add the folders you want to compare and consolidate.</p>}</div></> : <><div className="setup-primary"><Folder size={30} /><span className="eyebrow">PRIMARY DESTINATION</span><h2>{setup.destination || "Select an existing folder"}</h2><p>Existing contents are retained. Only additions approved in the merge plan will be copied here.</p><label><span>Use a listed source folder as primary</span><select aria-label="Primary destination from source folders" disabled={locked} value={setup.sources.some(source => source.path === setup.destination) ? setup.destination : ""} onChange={event => { if (event.target.value) void persist({ ...setup, destination: event.target.value }); }}><option value="">Select a listed folder…</option>{setup.sources.map(source => <option key={source.path} value={source.path}>{source.label} — {source.path}</option>)}</select></label><button className="primary" disabled={locked} onClick={() => void pick("destination")}><Folder size={15} /> Choose existing folder…</button></div><div className="setup-review"><h2>Ready to inventory</h2><p><strong>{incomingSources(setup).length}</strong> incoming source folders · the primary itself is excluded. Source and destination folders must otherwise be separate.</p>{incomingSources(setup).map(source => <p key={source.path}>{source.path}</p>)}{setupProblem(setup) && <p className="inline-error">{setupProblem(setup)}</p>}<p>Starting creates a saved job and reads the folder inventory. Copying comes later, after review and approval.</p></div></>}</section>
    <footer className="wizard-actions"><span role="status">{saving ? "Saving setup…" : error ? "Setup needs attention" : "Setup saved locally"}</span><div>{stage === "destination" && <button disabled={locked} onClick={() => setStage("sources")}><ArrowLeft size={14} /> Back to sources</button>}{stage === "sources" ? <button className="primary" disabled={locked || saving || !setup.name.trim() || !setup.sources.length || Boolean(error)} onClick={() => void next()}>Next: destination <ArrowRight size={14} /></button> : <button className="primary" disabled={locked || saving || Boolean(setupProblem(setup)) || Boolean(error)} onClick={() => void start()}>{busy ? <LoaderCircle size={14} className="spin" /> : <Play size={14} />}Start scan</button>}</div></footer>
  </main>;
}
