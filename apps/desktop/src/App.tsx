import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertCircle, Check, ChevronRight, File, Folder, FolderPlus, Layers, Link, LoaderCircle, Pause, Play, RefreshCw, RotateCcw, Search, ShieldCheck, X } from "lucide-react";
import { api } from "./api";
import folderloomIcon from "./assets/folderloom.png";
import type { Bootstrap, Comparison, Entry, Snapshot, Source } from "./api";
import { ancestors, buildInventoryTree, entryName, inventoryRows, latestComparisons, matchingContext } from "./inventory-tree";
import type { InventoryNode } from "./inventory-tree";
import { fullHashLocations } from "./matching-locations";
import { MatchingLocationList } from "./MatchingLocationList";

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
  const [sources, setSources] = useState<Source[]>([]);
  const [chosen, setChosen] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("all");
  const [rootFilter, setRootFilter] = useState<number | null>(null);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [showGaps, setShowGaps] = useState(false);
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
      const result = await api.bootstrap();
      if (version !== requestVersion.current) return;
      setBootstrap(result); setSnapshot(result.snapshot);
      const allSources = [...result.manifest.roots];
      for (const root of result.snapshot.roots) if (!allSources.some(source => root.input_path !== null ? sourceInput(source) === root.input_path : source.input_path === null && source.path === root.path)) allSources.push({ path: root.path, input_path: root.input_path, label: `Copy ${allSources.length + 1}` });
      setSources(allSources); setChosen(allSources.map(sourceInput).filter((path): path is string => path !== null));
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
    requestVersion.current++; setBusy(true); setError(null);
    try { setSnapshot(await action()); }
    catch (e) { setError(message(e)); }
    finally { setBusy(false); }
  }
  async function addFolder() {
    setBusy(true); setError(null);
    try {
      const path = await open({ directory: true, multiple: false, title: "Choose a source folder" });
      if (typeof path === "string") {
        if (!sources.some(source => source.path === path)) setSources(previous => [...previous, { path, label: `Copy ${previous.length + 1}` }]);
        setChosen(previous => previous.includes(path) ? previous : [...previous, path]);
      }
    } catch (e) { setError(message(e)); }
    finally { setBusy(false); }
  }
  const active = Boolean(snapshot?.worker);
  const blocked = active || busy || loading;
  const inventory = snapshot?.inventory_progress;
  const comparison = snapshot?.comparison_progress;
  const inventoryFinished = inventory && ["complete", "finished_with_errors"].includes(inventory.state);
  const recoveryRequired = Boolean(snapshot?.job_id && snapshot.recovery_required);
  const resumeInventory = snapshot?.job_id && inventory && ["paused", "running"].includes(inventory.state);
  const resumeComparison = comparison && ["paused", "running"].includes(comparison.state);
  const roots = snapshot?.roots ?? [];
  const rootName = (id: number) => { const root = roots.find(root => root.id === id); return sources.find(source => root?.input_path ? sourceInput(source) === root.input_path : source.path === root?.path)?.label ?? `Copy ${roots.findIndex(root => root.id === id) + 1}`; };
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
    setSelectedKey(key); setRevealKey(key);
  }
  const selected = snapshot?.entries.find(entry => entry.key === selectedKey);
  const selectedComparison = selected ? records.get(selected.key) : undefined;
  const placements = matching(selectedComparison);
  const failures = snapshot?.failures ?? [];
  const rootFailures = snapshot?.root_failures ?? [];
  const comparisonFailures = (snapshot?.comparisons ?? []).filter(record => record.failure);
  const currentGaps = (inventory?.failures ?? 0) + (comparison?.failures ?? 0) + (inventory?.stale_entries ?? 0);
  const currentState = active ? (snapshot?.pause_requested ? "Saving pause…" : snapshot?.worker === "scan" ? "Scanning folders" : "Comparing contents") : recoveryRequired ? "Saved job needs recovery" : resumeComparison ? "Comparison interrupted or paused" : resumeInventory ? "Inventory interrupted or paused" : comparison && comparison.state !== "not_started" ? stateLabel(comparison.state) : inventory ? stateLabel(inventory.state) : "Ready to scan";

  return <div className="app-shell">
    <aside className="sidebar" aria-label="Sources and saved job">
      <div className="brand"><span className="brand-mark"><img src={folderloomIcon} alt="" /></span><strong>Folderloom</strong><span className="demo-label">DEMO</span></div>
      <div className="sidebar-heading">SOURCE FOLDERS <span>{sources.length}</span></div>
      <div className="sources">
        {sources.map(source => { const inputPath = sourceInput(source); return <label className={`source ${chosen.includes(inputPath ?? "") ? "chosen" : ""}`} key={source.path}>
          <input type="checkbox" checked={chosen.includes(inputPath ?? "")} disabled={blocked || inputPath === null} onChange={e => { if (inputPath !== null) setChosen(previous => e.target.checked ? [...previous, inputPath] : previous.filter(path => path !== inputPath)); }} />
          <Folder size={17} /><span><strong>{source.label}</strong><small title={source.path}>{source.path}</small></span>
        </label>; })}
        {!sources.length && <p className="sidebar-empty">Choose the folders you want to inspect. Their contents stay in place.</p>}
      </div>
      <button className="add-source" disabled={blocked} onClick={() => void addFolder()}><FolderPlus size={15} /> Add folder</button>
      <button className="new-scan" disabled={blocked || !chosen.length} onClick={() => { setSelectedKey(null); setRootFilter(null); void act(() => api.start(chosen)); }}><Play size={14} /> Scan selected folders <span>{chosen.length}</span></button>
      <p className="sidebar-hint">Starts a new saved job. Previous job files are retained locally.</p>
      <div className="saved-job">
        <div className="sidebar-heading">CURRENT SAVED JOB</div>
        {snapshot?.job_id ? <><div className="job-line"><span className={`status-dot ${active ? "working" : ""}`} />{currentState}</div><code className="job-id" title={snapshot.job_id}>{snapshot.job_id}</code><p>{roots.length} source folders · Generation {inventory?.generation ?? "—"}</p></> : <p>No saved job yet.</p>}
        <button disabled={blocked} onClick={() => void load()}><RotateCcw size={14} /> Reopen saved job</button>
      </div>
      <div className="local-note"><ShieldCheck size={16} /><span>Local, read-only review<small>Files stay on this Mac or connected storage.</small></span></div>
    </aside>

    <main>
      <header className="toolbar"><div><div className="eyebrow">WORKSPACE</div><h1>{bootstrap?.manifest.title || "Folder comparison"}</h1></div><div className="toolbar-actions">
        <button disabled={blocked || !snapshot?.job_id} onClick={() => void act(() => api.run("scan"))}>{resumeInventory || recoveryRequired ? <Play size={15} /> : <RefreshCw size={15} />}{recoveryRequired ? "Recover / Resume" : resumeInventory ? "Resume scan" : "Re-scan"}</button>
        <button className="primary" disabled={blocked || !inventoryFinished} onClick={() => void act(() => api.run("compare"))}><Layers size={15} />{resumeComparison ? "Resume comparison" : "Compare contents"}</button>
        {active && <button disabled={busy || snapshot?.pause_requested} onClick={() => void act(api.pause)}><Pause size={15} />{snapshot?.pause_requested ? "Pausing…" : "Pause"}</button>}
      </div></header>
      {(error || snapshot?.error) && <div className="alert error" role="alert"><AlertCircle size={17} /><div><strong>Unable to finish this action</strong><p>{error || snapshot?.error}</p><small>{recoveryRequired ? "Choose Recover / Resume to recover the saved job and revalidate its sources. If recovery fails, check the reported storage or folder access problem and retry." : "Check folder access, then retry the action or reopen the saved job."}</small></div><button aria-label="Dismiss error" onClick={() => { setError(null); setSnapshot(previous => previous ? { ...previous, error: null } : previous); }}><X size={15} /></button></div>}
      {bootstrap?.configuration_error && <details className="configuration"><summary><AlertCircle size={14} /> Demo configuration unavailable — choose folders to begin</summary><p>{bootstrap.configuration_error}</p><code>{bootstrap.manifest_path}</code></details>}
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
      <footer className="workspace-footer"><span><ShieldCheck size={13} /> Read-only inventory & comparison</span><span>Samples are candidates. Full hashes are observations. Every placement is retained.</span></footer>
    </main>
  </div>;
}
