import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertCircle, Check, ChevronRight, File, Folder, Layers, Link, LoaderCircle, Pause, Play, RefreshCw, Search, ShieldCheck, X } from "lucide-react";
import { api } from "./api";
import folderloomIcon from "./assets/folderloom.png";
import type { Bootstrap, Comparison, Entry, Snapshot, Source, JobSetup, JobSummary } from "./api";
import { ancestors, buildInventoryTree, entryName, inventoryRows, latestComparisons, matchingContext } from "./inventory-tree";
import type { InventoryNode } from "./inventory-tree";
import { fullHashLocations } from "./matching-locations";
import { MatchingLocationList } from "./MatchingLocationList";
import { Consolidation, folderName } from "./Consolidation";
import type { WorkspaceStep } from "./Consolidation";
import { JobsHome } from "./JobsHome";
import { JobWizard } from "./JobWizard";
import { emptySetup, resumeStep } from "./job-workflow";

const stageLabels: Record<string, string> = {
  pending_sample: "Awaiting sample", sampling: "Reading sample", sampled: "Sample candidate",
  pending_full: "Awaiting full hash", hashing: "Reading full file", full_hash: "Full hash saved",
  unique_size: "Unique size", unique_sample: "Unique sample", failed: "Read failed",
};
const stateLabel = (value: string) => ({ running: "In progress", paused: "Paused", complete: "Complete", finished_with_errors: "Finished with gaps", not_started: "Not started", invalidated: "Needs comparison" }[value] ?? value);
function size(value: number) { if (value < 1024) return `${value} B`; const n = Math.min(Math.floor(Math.log(value) / Math.log(1024)), 4); return `${(value / 1024 ** n).toLocaleString(undefined, { maximumFractionDigits: 1 })} ${["B", "KB", "MB", "GB", "TB"][n]}`; }
const count = (n: number) => n.toLocaleString();
function message(error: unknown) { return error instanceof Error ? error.message : String(error); }
function sourceInput(source: Source) { return source.input_path === undefined ? source.path : source.input_path; }
function modified(value: string | null) { if (!value) return "Not recorded"; const date = new Date(Number(BigInt(value) / 1_000_000n)); return Number.isNaN(date.getTime()) ? "Not recorded" : date.toLocaleString(); }
function EntryIcon({ entry }: { entry: Entry }) { const Icon = entry.entry_type === "directory" ? Folder : entry.entry_type === "symlink" ? Link : File; return <Icon size={15} aria-hidden="true" />; }
function stage(entry: Entry, comparison?: Comparison) {
  if (entry.stale || comparison?.stale) return "Stale observation";
  if (entry.excluded) return "Excluded";
  if (comparison) return stageLabels[comparison.stage] ?? comparison.stage;
  if (entry.entry_type === "directory") return entry.directory_complete ? "Folder inventoried" : "Folder incomplete";
  if (entry.entry_type === "symlink") return "Link recorded";
  return "Inventoried";
}

export default function App() {
  const [bootstrap, setBootstrap] = useState<Bootstrap | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [screen, setScreen] = useState<"home" | "setup" | "job">("home");
  const [jobs, setJobs] = useState<JobSummary[]>([]);
  const [draft, setDraft] = useState<JobSetup | null>(null);
  const [wizardSetup, setWizardSetup] = useState<JobSetup>(emptySetup);
  const [wizardKey, setWizardKey] = useState(0);
  const sources: Source[] = useMemo(() => snapshot?.setup?.sources.map(source => ({ ...source, input_path: source.path })) ?? snapshot?.roots.map(root => ({ label: folderName(root.path), path: root.path, input_path: root.input_path })) ?? [], [snapshot?.setup, snapshot?.roots]);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [homeError, setHomeError] = useState<string | null>(null);
  const catalogVersion = useRef(0);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("all");
  const [rootFilter, setRootFilter] = useState<number | null>(null);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [showGaps, setShowGaps] = useState(false);
  const [primaryDestination, setPrimaryDestination] = useState("");
  const selectPrimary = useCallback((path: string) => setPrimaryDestination(path), []);
  const [step, setStep] = useState<WorkspaceStep>("inventory");
  const [openMatches, setOpenMatches] = useState<Set<string>>(() => new Set());
  const [expansion, setExpansion] = useState<Map<string, boolean>>(() => new Map());
  const [filteredCollapsed, setFilteredCollapsed] = useState<Set<string>>(() => new Set());
  const [revealKey, setRevealKey] = useState<string | null>(null);
  const rowRefs = useRef(new Map<string, HTMLTableRowElement>());
  const requestVersion = useRef(0);
  const searchRef = useRef<HTMLInputElement>(null);

  const load = useCallback(async () => {
    const version = ++requestVersion.current;
    setLoading(true); setError(null);
    try {
      const results = await Promise.allSettled([api.bootstrap(), api.jobs(), api.draft()]);
      if (version !== requestVersion.current) return;
      const [boot, catalog, savedDraft] = results;
      if (boot.status === "fulfilled") {
        setBootstrap(boot.value); setSnapshot(boot.value.snapshot);
        setPrimaryDestination(boot.value.snapshot.plan?.destination_input ?? boot.value.snapshot.setup?.destination ?? "");
      }
      if (catalog.status === "fulfilled") setJobs(catalog.value);
      if (savedDraft.status === "fulfilled") setDraft(savedDraft.value);
      const failures = results.flatMap((result, index) => result.status === "rejected" ? [`${["Saved workspace", "Job list", "Unfinished setup"][index]}: ${message(result.reason)}`] : []);
      setHomeError(failures.length ? failures.join(" · ") : null);
      setSelectedKey(null); setRootFilter(null);
    } catch (e) { if (version === requestVersion.current) setError(message(e)); }
    finally { if (version === requestVersion.current) setLoading(false); }
  }, []);
  useEffect(() => { void load(); return () => { requestVersion.current++; }; }, [load]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => { if ((event.metaKey || event.ctrlKey) && event.key === "f") { event.preventDefault(); searchRef.current?.focus(); } };
    window.addEventListener("keydown", onKey); return () => window.removeEventListener("keydown", onKey);
  }, []);
  // Only one in-flight poll. A command invalidates older responses so a pause/start
  // cannot be visually overwritten by a snapshot obtained before that command.
  useEffect(() => {
    if (!snapshot?.worker || busy) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      const version = requestVersion.current;
      try { const next = await api.snapshot(); if (!cancelled && version === requestVersion.current) { setSnapshot(next); setError(null); } }
      catch (e) { if (!cancelled && version === requestVersion.current) setError(message(e)); }
      if (!cancelled) timer = setTimeout(poll, 600);
    };
    timer = setTimeout(poll, 600);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [snapshot?.worker, busy]);

  async function act(action: () => Promise<Snapshot>) {
    catalogVersion.current++;
    requestVersion.current++; setBusy(true); setError(null);
    try { setSnapshot(await action()); return true; }
    catch (e) { setError(message(e)); return false; }
    finally { setBusy(false); }
  }
  function resetJobView(next: Snapshot) {
    setSnapshot(next); setSelectedKey(null); setRootFilter(null); setQuery(""); setFilter("all");
    setRevealKey(null); setShowGaps(false); setOpenMatches(new Set()); setExpansion(new Map()); setFilteredCollapsed(new Set());
    setPrimaryDestination(next.plan?.destination_input ?? next.setup?.destination ?? "");
    setStep(resumeStep(next)); setScreen("job");
  }
  async function refreshJobs() {
    const version = ++catalogVersion.current;
    setLoading(true); setHomeError(null);
    const results = await Promise.allSettled([api.jobs(), api.draft()]);
    if (version !== catalogVersion.current) return;
    const [catalog, savedDraft] = results;
    if (catalog.status === "fulfilled") setJobs(catalog.value);
    if (savedDraft.status === "fulfilled") setDraft(savedDraft.value);
    else setDraft(null);
    const failures = results.flatMap((result, index) => result.status === "rejected" ? [`${index ? "Unfinished setup" : "Job list"}: ${message(result.reason)}`] : []);
    setHomeError(failures.length ? failures.join(" · ") : null); setLoading(false);
  }
  async function setJobArchived(id: string, archived: boolean) {
    setBusy(true); setError(null); setHomeError(null);
    try { await api.setJobArchived(id, archived); await refreshJobs(); }
    catch (failure) { setError(message(failure)); }
    finally { setBusy(false); }
  }
  async function deleteJob(id: string) {
    setBusy(true); setError(null); setHomeError(null);
    try { setSnapshot(await api.deleteJob(id)); await refreshJobs(); }
    finally { setBusy(false); }
  }
  async function cancelSetup() {
    setBusy(true); setError(null);
    try { await api.clearDraft(); setDraft(null); await showHome(); }
    finally { setBusy(false); }
  }
  async function showHome() { setScreen("home"); await refreshJobs(); }
  async function openJob(id: string) {
    catalogVersion.current++; setLoading(false); setHomeError(null);
    if (snapshot?.worker) { if (snapshot.job_id === id) { setScreen("job"); return; } setError("Pause or finish the working job before opening another one."); return; }
    const version = ++requestVersion.current; setBusy(true); setError(null);
    try { const next = await api.openJob(id); if (version === requestVersion.current) resetJobView(next); }
    catch (failure) { setError(message(failure)); }
    finally { if (version === requestVersion.current) setBusy(false); }
  }
  async function beginSetup(setup: JobSetup) {
    requestVersion.current++; catalogVersion.current++; setLoading(false); setHomeError(null);
    if (snapshot?.worker) return;
    setBusy(true); setError(null);
    try { await api.saveDraft(setup); setDraft(setup); setWizardSetup(setup); setWizardKey(key => key + 1); setScreen("setup"); }
    catch (failure) { setError(message(failure)); }
    finally { setBusy(false); }
  }
  async function cloneJob() {
    if (snapshot?.worker) return;
    if (!snapshot?.setup && snapshot?.roots.some(root => root.input_path === null)) { setError("This job contains paths that cannot be reused as text. Start a new job and choose its source folders directly."); return; }
    const setup = snapshot?.setup ?? { name: "Folder collection", sources: sources.map(source => ({ label: source.label, path: sourceInput(source)! })), destination: primaryDestination };
    await beginSetup({ ...setup, name: `${setup.name} — new job`, sources: setup.sources.map(source => ({ ...source })), destination: snapshot?.plan?.destination_input ?? setup.destination });
  }
  async function createJob(setup: JobSetup) {
    catalogVersion.current++; setLoading(false); setHomeError(null);
    const version = ++requestVersion.current; setBusy(true); setError(null);
    try { const next = await api.createJob(setup); if (version !== requestVersion.current) return false; resetJobView(next); setStep("inventory"); setDraft(null); return true; }
    catch (failure) { setError(message(failure)); return false; }
    finally { if (version === requestVersion.current) setBusy(false); }
  }
  useEffect(() => { if (screen === "home" && !snapshot?.worker && !loading) void refreshJobs(); }, [screen, snapshot?.worker]);
  const active = Boolean(snapshot?.worker);
  const blocked = active || busy || loading;
  const inventory = snapshot?.inventory_progress;
  const comparison = snapshot?.comparison_progress;
  const inventoryFinished = inventory && ["complete", "finished_with_errors"].includes(inventory.state);
  const recoveryRequired = Boolean(snapshot?.job_id && snapshot.recovery_required);
  const resumeInventory = snapshot?.job_id && inventory && ["paused", "running"].includes(inventory.state);
  const resumeComparison = comparison && ["paused", "running"].includes(comparison.state);
  const roots = snapshot?.roots ?? [];
  const rootName = (id: number) => { const root = roots.find(root => root.id === id); return sources.find(source => root?.input_path ? sourceInput(source) === root.input_path : source.path === root?.path)?.label ?? folderName(root?.path ?? "Source folder"); };
  const records = useMemo(() => latestComparisons(snapshot?.comparisons ?? []), [snapshot?.comparisons]);
  const matchGroups = useMemo(() => fullHashLocations(snapshot?.entries ?? [], snapshot?.comparisons ?? []), [snapshot?.entries, snapshot?.comparisons]);
  const matching = (record?: Comparison) => record ? matchGroups.get(record.key) ?? [] : [];
  const visible = useMemo(() => (snapshot?.entries ?? []).filter(entry => {
    const record = records.get(entry.key);
    if (rootFilter !== null && entry.root_id !== rootFilter) return false;
    if (query && !`${entry.relative_path} ${rootName(entry.root_id)}`.toLocaleLowerCase().includes(query.toLocaleLowerCase())) return false;
    if (filter === "files") return entry.entry_type === "file";
    if (filter === "matches") return matching(record).length > 1 && !entry.stale;
    if (filter === "unique") return !entry.stale && !record?.stale && ["unique_size", "unique_sample"].includes(record?.stage ?? "");
    if (filter === "attention") return entry.stale || record?.stale || record?.stage === "failed" || (entry.entry_type === "directory" && !entry.directory_complete);
    return true;
  }), [snapshot?.entries, records, matchGroups, rootFilter, query, filter, sources, roots]);
  const tree = useMemo(() => buildInventoryTree(snapshot?.roots ?? [], snapshot?.entries ?? []), [snapshot?.roots, snapshot?.entries]);
  const filtering = Boolean(query || filter !== "all");
  const included = useMemo(() => filtering ? matchingContext(tree, new Set(visible.map(entry => entry.key))) : undefined, [tree, visible, filtering]);
  const isExpanded = (node: InventoryNode) => filtering ? !filteredCollapsed.has(node.id) : expansion.get(node.id) ?? node.source;
  const rows = inventoryRows(tree, isExpanded, included, rootFilter);
  useEffect(() => { setExpansion(new Map()); setFilteredCollapsed(new Set()); setOpenMatches(new Set()); }, [snapshot?.job_id]);
  useEffect(() => { setFilteredCollapsed(new Set()); }, [query, filter, rootFilter]);
  useEffect(() => {
    if (!revealKey) return;
    rowRefs.current.get(revealKey)?.scrollIntoView({ block: "nearest" });
    setRevealKey(null);
  }, [revealKey]);
  function toggleFolder(node: InventoryNode) {
    if (filtering) setFilteredCollapsed(previous => { const next = new Set(previous); if (next.has(node.id)) next.delete(node.id); else next.add(node.id); return next; });
    else setExpansion(previous => new Map(previous).set(node.id, !(previous.get(node.id) ?? node.source)));
  }
  function expandAll(expanded: boolean) {
    if (filtering) setFilteredCollapsed(new Set(expanded ? [] : tree.folders.map(node => node.id)));
    else setExpansion(new Map(tree.folders.map(node => [node.id, expanded])));
  }
  function revealPlacement(key: string) {
    const node = tree.byEntry.get(key);
    if (!node) return;
    setQuery(""); setFilter("all"); setRootFilter(null);
    setExpansion(previous => { const next = new Map(previous); for (const parent of ancestors(node)) next.set(parent.id, true); return next; });
    setStep("inventory"); setSelectedKey(key); setRevealKey(key);
  }
  const selected = snapshot?.entries.find(entry => entry.key === selectedKey);
  const selectedComparison = selected ? records.get(selected.key) : undefined;
  const placements = matching(selectedComparison);
  const failures = snapshot?.failures ?? [];
  const rootFailures = snapshot?.root_failures ?? [];
  const comparisonFailures = (snapshot?.comparisons ?? []).filter(record => record.failure);
  const currentGaps = (inventory?.failures ?? 0) + (comparison?.failures ?? 0) + (inventory?.stale_entries ?? 0);
  const savedPlanState = snapshot?.plan ? ({ proposed: "Merge plan ready for review", approved: "Merge plan approved", running: "Copy interrupted — resume in Results", paused: "Copy paused", complete: "Copy results ready", finished_with_errors: "Copy results need review" }[snapshot.plan.state]) : null;
  const currentState = active ? (snapshot?.pause_requested ? "Saving pause…" : snapshot?.worker === "scan" ? "Reading folder inventory" : snapshot?.worker === "plan" ? "Building merge plan" : snapshot?.worker === "execute" ? "Copying approved additions" : "Comparing file contents") : recoveryRequired ? "Saved job needs recovery" : savedPlanState ?? (resumeComparison ? "Comparison interrupted or paused" : resumeInventory ? "Inventory interrupted or paused" : comparison && comparison.state !== "not_started" ? stateLabel(comparison.state) : inventory ? stateLabel(inventory.state) : "Ready to scan");

  if (screen !== "job") return <div className="app-shell jobs-shell"><aside className="sidebar"><div className="brand"><span className="brand-mark"><img src={folderloomIcon} alt="" /></span><strong>Folderloom</strong></div><p className="sidebar-hint">Local jobs. Saved progress. Sources stay intact.</p><div className="local-note"><ShieldCheck size={17} /><span>On your computer<small>No account needed.</small></span></div></aside>{screen === "home" ? <JobsHome jobs={jobs} loading={loading} busy={busy} workingJob={snapshot?.worker ? snapshot.job_id : jobs.find(job => job.active)?.id ?? null} error={homeError || error} hasDraft={Boolean(draft)} onNew={() => void beginSetup(emptySetup())} onDraft={() => { if (draft) void beginSetup(draft); }} onOpen={id => void openJob(id)} onRefresh={() => void refreshJobs()} onArchive={(id, archived) => void setJobArchived(id, archived)} onDelete={deleteJob} onDiscardDraft={() => void cancelSetup().catch(failure => setError(message(failure)))} /> : <><JobWizard key={wizardKey} initial={wizardSetup} busy={busy} onCancel={cancelSetup} onSaveClose={() => void showHome()} onStart={createJob} />{error && <div className="wizard-external-error alert error" role="alert">{error}</div>}</>}</div>;

  return <div className="app-shell">
    <aside className="sidebar" aria-label="Job sources and saved progress">
      <div className="brand"><span className="brand-mark"><img src={folderloomIcon} alt="" /></span><strong>Folderloom</strong></div>
      <button className="back-to-jobs" disabled={busy} onClick={() => void showHome()}>← All jobs</button>
      <div className="sidebar-heading">SAVED SOURCE FOLDERS <span>{roots.length}</span></div>
      <div className="sources">{roots.map(root => <div className="source" key={root.id}><Folder size={17} /><span><strong>{rootName(root.id)}</strong><small title={root.path}>{root.path}</small></span></div>)}</div>
      <p className="sidebar-hint">Sources are fixed for this saved job. To add or remove folders, use its setup for a new job.</p>
      <button disabled={blocked} onClick={() => void cloneJob()}>Use as new job</button>
      <div className="saved-job"><div className="sidebar-heading">CURRENT JOB</div><div className="job-line"><span className={`status-dot ${active ? "working" : ""}`} />{currentState}</div><p>{snapshot?.setup?.name || "Saved folder job"}</p><code className="job-id" title={snapshot?.job_id ?? ""}>{snapshot?.job_id}</code><p>{roots.length} source folders · Generation {inventory?.generation ?? "—"}</p></div>
      <div className="local-note"><ShieldCheck size={16} /><span>Your sources stay intact<small>Copy only reviewed additions to your destination.</small></span></div>
    </aside>

    <main>
      <header className="toolbar"><div><div className="eyebrow">WORKSPACE</div><h1>{snapshot?.setup?.name || "Saved folder job"}</h1></div><div className="toolbar-actions">
        {step === "inventory" && <><button disabled={blocked || !snapshot?.job_id} onClick={() => void act(() => api.run("scan"))}>{resumeInventory || recoveryRequired ? <Play size={15} /> : <RefreshCw size={15} />}{recoveryRequired ? "Recover / Resume" : resumeInventory ? "Resume scan" : "Re-scan"}</button>
        <button className="primary" disabled={blocked || !inventoryFinished} onClick={() => void act(() => api.run("compare"))}><Layers size={15} />{resumeComparison ? "Resume comparison" : "Compare contents"}</button></>}
        {active && snapshot?.worker !== "plan" && <button disabled={busy || snapshot?.pause_requested} onClick={() => void act(api.pause)}><Pause size={15} />{snapshot?.pause_requested ? "Pausing…" : "Pause"}</button>}
      </div></header>
      <nav className="workspace-steps" aria-label="Consolidation steps"><span className="completed-step">1 Sources</span><span className="completed-step">2 Destination</span>{(["inventory", "plan", "results"] as const).map((item, index) => <button key={item} disabled={item === "plan" ? !snapshot?.plan && inventory?.state !== "complete" : item === "results" ? !snapshot?.plan : false} aria-current={step === item ? "step" : undefined} onClick={() => setStep(item)}><span>{index + 3}</span>{item === "inventory" ? "Scan & compare" : item === "plan" ? "Merge plan" : "Results"}{item === "plan" && snapshot?.plan && <small>Revision {snapshot.plan.revision}</small>}</button>)}</nav>
      {(error || snapshot?.error) && <div className="alert error" role="alert"><AlertCircle size={17} /><div><strong>Unable to finish this action</strong><p>{error || snapshot?.error}</p><small>{recoveryRequired ? "Choose Recover / Resume to recover the saved job and revalidate its sources. If recovery fails, check the reported storage or folder access problem and retry." : "Check folder access, then retry the action or reopen the saved job."}</small></div><button aria-label="Dismiss error" onClick={() => { setError(null); setSnapshot(previous => previous ? { ...previous, error: null } : previous); }}><X size={15} /></button></div>}
      <Consolidation snapshot={snapshot} sources={sources} onDestinationChange={selectPrimary} busy={busy || loading} step={step} rootName={rootName} act={act} onStep={setStep} onError={setError} onReveal={revealPlacement} />
      <div className="inventory-view" hidden={step !== "inventory"}>
      <section className="progress-strip" aria-label="Saved progress" aria-busy={active}>
        <div className="progress-state">{active ? <LoaderCircle className="spin" size={19} /> : currentGaps ? <AlertCircle size={19} /> : <Check size={19} />}<span><strong>{currentState}</strong><small>{active ? "Saving progress at each checkpoint" : recoveryRequired ? "Recover / Resume revalidates the saved sources" : snapshot?.job_id ? "Saved observations · re-scan to refresh" : "Select source folders, then start a scan"}</small></span></div>
        <div className="metric"><strong>{count(inventory?.entries ?? 0)}</strong><span>entries saved</span></div>
        <div className="metric"><strong>{count(comparison?.full_hashes ?? 0)}</strong><span>full hashes</span></div>
        <div className="metric"><strong>{count((comparison?.unique_sizes ?? 0) + (comparison?.unique_samples ?? 0))}</strong><span>unique candidates</span></div>
        <button className={`gap-count ${currentGaps ? "has-gaps" : ""}`} disabled={!failures.length && !rootFailures.length && !comparisonFailures.length && !currentGaps} onClick={() => { setShowGaps(value => !value); if (inventory?.stale_entries) setFilter("attention"); }}><strong>{count(currentGaps)}</strong><span>gaps / stale <ChevronRight size={12} /></span></button>
      </section>
      {(active || comparison?.bytes_read) ? <div className="work-progress"><span>{snapshot?.worker === "scan" ? `${count(inventory?.pending_directories ?? 0)} folders awaiting enumeration` : `${count(comparison?.pending_files ?? 0)} files awaiting comparison`}</span><span>{size(comparison?.bytes_read ?? 0)} content read{inventory?.failed_directories ? ` · ${inventory.failed_directories} incomplete folders` : ""}</span></div> : null}
      {showGaps && <section className="gap-panel" aria-label="Recorded failures"><div className="gap-heading"><strong>Recorded gaps and failures</strong><button onClick={() => setShowGaps(false)} aria-label="Close failures"><X size={14} /></button></div><p>Restore access and re-scan to create fresh observations. Earlier-generation failures remain as history.</p>{!failures.length && !rootFailures.length && !comparisonFailures.length && <p>Stale entries are shown in the Needs attention filter below.</p>}{rootFailures.map((failure, index) => <div className="failure-row" key={`root-${index}`}><strong>{rootName(failure.root_id)} · root comparison gap</strong><span>{failure.message}</span><small>Generation {failure.generation}{failure.stale ? " · stale" : ""}</small></div>)}{comparisonFailures.map(failure => <div className="failure-row" key={`comparison-${failure.key}-${failure.generation}`}><strong>{rootName(failure.root_id)} / {failure.relative_path} · content read</strong><span>{failure.failure}</span><small>Generation {failure.generation}{failure.stale ? " · historical / stale" : ""}</small></div>)}{failures.map((failure, index) => <div className="failure-row" key={`file-${index}`}><strong>{rootName(failure.root_id)} / {failure.relative_path || "(source root)"}</strong><span>{failure.message}</span><small>Generation {failure.generation}{failure.generation !== inventory?.generation ? " · historical" : ""}</small></div>)}</section>}
      <div className="inventory-toolbar"><div className="inventory-title"><h2>Folder inventory</h2><span>{count(visible.length)} {filtering ? "matching placements" : "placements"}</span></div><label className="search"><Search size={15} /><input ref={searchRef} type="search" placeholder="Search paths…" aria-label="Search inventory paths" value={query} onChange={e => setQuery(e.target.value)} /><kbd>⌘ F</kbd></label><select aria-label="Filter inventory" value={filter} onChange={e => setFilter(e.target.value)}><option value="all">All entries</option><option value="files">Files only</option><option value="matches">Full-hash matches</option><option value="unique">Unique candidates</option><option value="attention">Needs attention</option></select><select aria-label="Filter source folder" value={rootFilter ?? "all"} onChange={e => setRootFilter(e.target.value === "all" ? null : Number(e.target.value))}><option value="all">All sources</option>{roots.map(root => <option key={root.id} value={root.id}>{rootName(root.id)}</option>)}</select></div>
      <div className={`inventory-workbench ${selected ? "with-details" : ""}`}>
        <div className="table-region"><div className="tree-toolbar"><span>{filtering ? "Matching entries with their parent folders" : "Open a folder to explore its saved contents"}</span><div><button onClick={() => expandAll(true)} disabled={!rows.length}>Expand all</button><button onClick={() => expandAll(false)} disabled={!rows.length}>Collapse all</button></div></div><div className="table-scroll"><table aria-label="Folder inventory"><thead><tr><th>Folder / file</th><th>Source</th><th className="numeric">Size</th><th>Evidence / stage</th></tr></thead><tbody>{rows.map(({ node, depth }) => {
          const entry = node.entry;
          const record = entry ? records.get(entry.key) : undefined;
          const locations = matching(record);
          const locationCount = locations.length;
          const matchesOpen = Boolean(entry && openMatches.has(entry.key));
          const matchesId = `matches-${encodeURIComponent(node.id)}`;
          const fullMatch = locationCount > 1;
          const folder = node.source || entry?.entry_type === "directory";
          const name = node.source ? rootName(node.rootId) : entryName(entry!);
          const selectedRow = Boolean(entry && selectedKey === entry.key);
          return <Fragment key={node.id}><tr ref={element => { if (entry) { if (element) rowRefs.current.set(entry.key, element); else rowRefs.current.delete(entry.key); } }} className={`${selectedRow ? "selected" : ""} ${node.source ? "source-root" : ""} ${filtering && entry && !visible.includes(entry) ? "ancestor-context" : ""}`} onClick={() => { if (entry) setSelectedKey(entry.key); }}>
            <td><div className="tree-name" style={{ paddingLeft: depth * 18 }}>{folder ? <button className="folder-toggle" aria-label={`${isExpanded(node) ? "Collapse" : "Expand"} ${name}`} aria-expanded={isExpanded(node)} onClick={event => { event.stopPropagation(); toggleFolder(node); }} onKeyDown={event => { if (event.key === "ArrowRight" || event.key === "ArrowLeft") { event.preventDefault(); if (isExpanded(node) !== (event.key === "ArrowRight")) toggleFolder(node); } }}><ChevronRight size={13} className={isExpanded(node) ? "expanded" : ""} /></button> : <span className="tree-spacer" />}<button className="file-select" aria-pressed={selectedRow} disabled={!entry} onClick={() => { if (entry) setSelectedKey(entry.key); }} title={entry?.relative_path || roots.find(root => root.id === node.rootId)?.path}>{entry ? <EntryIcon entry={entry} /> : <Folder size={15} />}<span>{name}</span></button></div></td><td className="source-cell">{rootName(node.rootId)}</td><td className="numeric">{entry?.entry_type === "file" ? size(entry.size) : "—"}</td><td>{fullMatch && entry ? <button id={`${matchesId}-toggle`} className="match-toggle" aria-expanded={matchesOpen} aria-controls={matchesId} aria-label={`${matchesOpen ? "Hide" : "Show"} ${locationCount} matching locations for ${name}`} onClick={event => { event.stopPropagation(); setOpenMatches(previous => { const next = new Set(previous); if (next.has(entry.key)) next.delete(entry.key); else next.add(entry.key); return next; }); }}><ChevronRight size={13} className={matchesOpen ? "expanded" : ""} /><span><strong>{count(locationCount)} matching locations</strong><small>Full-content hash matches</small></span></button> : <span className={`stage ${entry?.stale || record?.stale || record?.stage === "failed" ? "warning" : ""}`}>{entry ? stage(entry, record) : "No saved root observation"}</span>}</td>
          </tr>{fullMatch && entry && matchesOpen && <tr className="matching-detail-row"><td colSpan={4}><section id={matchesId} className="inline-matches" aria-label={`Matching locations for ${name}`}><div className="inline-matches-heading"><h3>{count(locationCount)} matching locations <span>for {name}</span></h3><button aria-label={`Close matching locations for ${name}`} onClick={() => { setOpenMatches(previous => { const next = new Set(previous); next.delete(entry.key); return next; }); document.getElementById(`${matchesId}-toggle`)?.focus(); }}><X size={14} /></button></div><p>Same full-content hash. These are saved locations; import decisions will be made in the merge plan.</p><MatchingLocationList locations={locations} roots={roots} rootName={rootName} currentKey={entry.key} currentLabel="This row" onReveal={revealPlacement} /><p className="matching-footnote">Locations include results outside the current filter. Overlapping source folders can refer to the same file.</p></section></td></tr>}</Fragment>;
        })}</tbody></table>
          {!rows.length && <div className="empty-state">{loading ? <LoaderCircle className="spin" size={27} /> : <Folder size={29} />}<h3>{loading ? "Opening saved workspace…" : query || filter !== "all" || rootFilter !== null ? "No placements match this view" : active ? "Waiting for the first saved entries" : snapshot?.job_id ? "No entries saved yet" : "Start with your source folders"}</h3><p>{loading ? "Reading local job records." : query || filter !== "all" || rootFilter !== null ? "Try a different search or show all entries." : "Scan selected folders to build an inventory, then compare their contents. Every discovered placement stays visible."}</p>{(query || filter !== "all" || rootFilter !== null) && <button onClick={() => { setQuery(""); setFilter("all"); setRootFilter(null); }}>Clear filters</button>}{error && !bootstrap && <button onClick={() => void load()}>Retry opening workspace</button>}</div>}
        </div><div className="table-footer"><span>{count(rows.length)} rows shown · {count(visible.length)} {filtering ? "matching placements" : "saved placements"}</span><span>Folders retain their full hierarchy</span></div></div>
        {selected && <aside className="details" aria-label="Selected file details"><div className="details-heading"><span>PLACEMENT DETAILS</span><button aria-label="Close file details" onClick={() => setSelectedKey(null)}><X size={15} /></button></div><div className="selected-name"><EntryIcon entry={selected} /><h3>{entryName(selected)}</h3></div><span className="stage">{stage(selected, selectedComparison)}</span><dl><dt>Source folder</dt><dd>{rootName(selected.root_id)}</dd><dt>Root path</dt><dd className="path">{roots.find(root => root.id === selected.root_id)?.path}</dd><dt>Relative placement</dt><dd className="path">{selected.relative_path || "(source root)"}</dd><dt>Size</dt><dd>{selected.entry_type === "file" ? <>{size(selected.size)} <span className="muted">({count(selected.size)} bytes)</span></> : selected.entry_type === "directory" ? "Not calculated for folders" : "Not a regular file"}</dd><dt>Modified observation</dt><dd>{modified(selected.modified_ns)}</dd>{selected.link_target && <><dt>Link target</dt><dd className="path">{selected.link_target}</dd></>}</dl>
          {(selected.stale || selectedComparison?.stale) && <p className="detail-warning">This observation is stale. Re-scan the source before relying on its evidence.</p>}
          {selectedComparison?.failure && <p className="detail-warning">{selectedComparison.failure}</p>}
          <div className="detail-section"><h4>Matching locations <span>{placements.length}</span></h4><MatchingLocationList locations={placements} roots={roots} rootName={rootName} currentKey={selected.key} currentLabel="Selected" onReveal={revealPlacement} />{!placements.length && <p>No other placement has the same current full-content hash.</p>}<p className="evidence-note">{placements.length > 0 && <>All locations with matching full-content hashes are listed, including this selection. Overlapping source roots can refer to the same file. </>}Saved hashes do not authorize copying or cleanup.</p></div>
          <div className="detail-section"><h4>Content evidence</h4>{selectedComparison ? <><dl><dt>Algorithm / generation</dt><dd>{selectedComparison.algorithm.toUpperCase()} · {selectedComparison.generation}</dd><dt>Sample · {selectedComparison.sample_format}</dt><dd className="hash">{selectedComparison.sample_hash || "No completed sample"}</dd><dt>Full-content hash</dt><dd className="hash">{selectedComparison.full_hash || "No completed full hash"}</dd><dt>Content read this attempt</dt><dd>{size(selectedComparison.bytes_read)}</dd></dl><p className="evidence-note">Samples only screen candidates. Unique-size and unique-sample files remain in the inventory.</p></> : <p>{selected.entry_type === "file" ? "Run Compare contents after the scan finishes to examine this file." : "Content comparison applies to regular files. This placement remains in the inventory."}</p>}</div>
        </aside>}
      </div>
      </div>
      <div className="job-step-actions"><button disabled={busy} onClick={() => step === "inventory" ? void showHome() : setStep(step === "results" ? "plan" : "inventory")}>← {step === "inventory" ? "All jobs" : step === "results" ? "Back to merge plan" : "Back to scan & compare"}</button>{step === "inventory" ? <button className="primary" disabled={blocked || inventory?.state !== "complete"} onClick={() => setStep("plan")}>Next: merge plan <ChevronRight size={14} /></button> : step === "plan" && snapshot?.plan ? <button disabled={busy} onClick={() => setStep("results")}>View results <ChevronRight size={14} /></button> : null}</div>
      <footer className="workspace-footer"><span><ShieldCheck size={13} /> Local inventory & consolidation</span><span>Review the plan. Copy approved additions. Keep your sources.</span></footer>
    </main>
  </div>;
}
