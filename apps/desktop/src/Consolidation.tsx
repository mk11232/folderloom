import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowRight, Check, Folder, FolderOpen, FolderPlus, LoaderCircle, Pause, Play, X } from "lucide-react";
import { api } from "./api";
import type { MappingRequest, MergePlan, Snapshot, Source } from "./api";

export type WorkspaceStep = "inventory" | "plan" | "results";
type DraftMapping = MappingRequest & { selected: boolean };
type Props = {
  snapshot: Snapshot | null;
  sources: Source[];
  onDestinationChange: (path: string) => void;
  busy: boolean;
  step: WorkspaceStep;
  rootName: (id: number) => string;
  act: (action: () => Promise<Snapshot>) => Promise<boolean>;
  onStep: (step: WorkspaceStep) => void;
  onError: (message: string) => void;
  onReveal: (key: string) => void;
};
export function folderName(path: string) { return path.replace(/\/+$/, "").split("/").at(-1) || path; }
export function displayPath(root: string, relative: string) { return relative ? `${root.replace(/\/+$/, "")}/${relative}` : root; }
export function pathsOverlap(first: string, second: string) {
  if (!first || !second) return false;
  const a = first.replace(/\/+$/, "") || "/";
  const b = second.replace(/\/+$/, "") || "/";
  return a === b || a === "/" || b === "/" || a.startsWith(`${b}/`) || b.startsWith(`${a}/`);
}
export function excludePrimaryMappings(mappings: DraftMapping[], entries: { key: string; root_id: number }[], roots: Snapshot["roots"], destination: string) {
  const excludedRoots = new Set(roots.filter(root => root.input_path !== null && pathsOverlap(root.input_path, destination)).map(root => root.id));
  const excludedKeys = new Set(entries.filter(entry => excludedRoots.has(entry.root_id)).map(entry => entry.key));
  return mappings.map(mapping => excludedKeys.has(mapping.source_key) ? { ...mapping, selected: false } : mapping);
}
const actionLabels = { copy: "Copy addition", already_present: "Already present", create_directory: "Create folder", blocked: "Needs review" };
const stateLabels = { proposed: "Ready for review", approved: "Approved for copying", running: "Copying and verifying", paused: "Copy paused", complete: "Finished", finished_with_errors: "Finished with exceptions" };

export function PlanSummary({ plan, results = false }: { plan: MergePlan; results?: boolean }) {
  const selected = plan.items.filter(item => item.included);
  const counts = results
    ? [[selected.filter(item => item.status === "verified" && item.action === "copy").length, "additions verified"], [selected.filter(item => item.status === "verified" && item.action === "already_present").length, "existing files verified"], [selected.filter(item => item.status === "verified" && item.action === "create_directory").length, "folders verified"], [plan.items.filter(item => item.status === "failed" || item.action === "blocked").length, "exceptions"], [plan.items.filter(item => !item.included && item.action !== "blocked").length, "excluded items"]]
    : [[selected.filter(item => item.action === "copy").length, "planned additions"], [selected.filter(item => item.action === "already_present").length, "already present"], [selected.filter(item => item.action === "create_directory").length, "planned folders"], [plan.items.filter(item => item.action === "blocked").length, "need review"], [plan.items.filter(item => !item.included && item.action !== "blocked").length, "excluded items"]];
  return <div className="plan-summary" aria-label={results ? "Verified results" : "Planned work"}>{counts.map(([count, label]) => <div key={label}><strong>{Number(count).toLocaleString()}</strong><span>{label}</span></div>)}</div>;
}

export function Consolidation({ snapshot, sources, onDestinationChange, busy, step, rootName, act, onStep, onError, onReveal }: Props) {
  const plan = snapshot?.plan;
  const roots = snapshot?.roots ?? [];
  const folders = snapshot?.entries.filter(entry => entry.entry_type === "directory") ?? [];
  const [destination, setDestination] = useState(plan?.destination_input ?? snapshot?.setup?.destination ?? "");
  const [mappings, setMappings] = useState<DraftMapping[] | null>(null);
  const [dirty, setDirty] = useState(false);
  const [choosing, setChoosing] = useState(false);
  const [itemFilter, setItemFilter] = useState("all");
  const [consent, setConsent] = useState(false);
  const active = Boolean(snapshot?.worker);
  const locked = busy || active || choosing;
  const sourcePath = (rootId: number, relative: string) => displayPath(roots.find(root => root.id === rootId)?.path ?? "Unknown source", relative);
  const inventoryReady = snapshot?.inventory_progress?.state === "complete";
  const generationChanged = Boolean(plan && snapshot?.inventory_progress && plan.generation !== snapshot.inventory_progress.generation);
  const needsRebuild = dirty || generationChanged;
  const decisionsEditable = plan?.state === "proposed" || plan?.state === "approved";

  // Hydrate drafts from the durable plan, not from browser storage. Decisions only
  // change the revision, so they must not discard edits to the next mapping draft.
  useEffect(() => {
    // Choosing the primary comes before scanning; starting a job retains it.
    const savedDestination = plan ? plan.destination_input ?? "" : snapshot?.setup?.destination ?? "";
    setDestination(savedDestination); onDestinationChange(savedDestination);
    setMappings(plan ? plan.mappings.map(mapping => ({ source_key: mapping.source_key, destination_relative: mapping.destination_input ?? "", selected: mapping.destination_input !== null })) : null);
    setDirty(false); setConsent(false);
  }, [snapshot?.job_id, plan?.id]);
  useEffect(() => { setConsent(false); }, [plan?.revision]);
  const candidates = excludePrimaryMappings(mappings ?? folders.filter(entry => entry.parent_key === null).map(entry => ({ source_key: entry.key, destination_relative: "", selected: false })), folders, roots, destination);
  const primaryRootIds = new Set(roots.filter(root => root.input_path !== null && pathsOverlap(root.input_path, destination)).map(root => root.id));
  const sourceOptions = sources.filter(source => source.input_path !== null).map(source => ({ ...source, input: source.input_path ?? source.path }));
  const selectedMappings = candidates.filter(mapping => mapping.selected);
  const invalidRelative = selectedMappings.some(mapping => /^(?:[\\/]|[A-Za-z]:)/.test(mapping.destination_relative) || mapping.destination_relative.split(/[\\/]/).includes("..") || mapping.destination_relative.includes("\0"));
  const invalidSource = selectedMappings.some(mapping => {
    const entry = folders.find(entry => entry.key === mapping.source_key);
    return !entry || entry.stale || entry.excluded || sourcePath(entry.root_id, entry.relative_path).replace(/\/$/, "") === destination.replace(/\/$/, "");
  });
  const selectedItems = plan?.items.filter(item => item.included) ?? [];
  const includedBlocked = selectedItems.some(item => item.action === "blocked");
  const approved = Boolean(plan && plan.approved_revision === plan.revision);
  const unfinished = selectedItems.filter(item => item.status !== "verified" && item.status !== "skipped" && item.action !== "blocked");
  const rows = plan?.items.filter(item => itemFilter === "all" || (itemFilter === "included" ? item.included : itemFilter === "exceptions" ? item.status === "failed" || item.action === "blocked" : item.status === "verified")) ?? [];
  function updateMapping(index: number, patch: Partial<DraftMapping>) {
    setMappings(candidates.map((mapping, itemIndex) => itemIndex === index ? { ...mapping, ...patch } : mapping));
    setDirty(true); setConsent(false);
  }
  function selectDestination(path: string) {
    setDestination(path); setMappings(excludePrimaryMappings(candidates, folders, roots, path));
    setDirty(true); setConsent(false); onDestinationChange(path);
  }
  async function chooseDestination() {
    setChoosing(true);
    try {
      const path = await open({ directory: true, multiple: false, title: "Choose the primary destination folder" });
      if (typeof path === "string") selectDestination(path);
    } catch (error) { onError(String(error)); }
    finally { setChoosing(false); }
  }
  async function buildPlan() {
    // A successful command starts a worker; only a newly saved plan can replace
    // this draft. Keep dirty state if planning fails and the old plan survives.
    setDirty(true); setConsent(false);
    await act(() => api.plan(destination, selectedMappings.map(({ source_key, destination_relative }) => ({ source_key, destination_relative }))));
  }
  async function execute() {
    if (plan && await act(() => api.execute(plan.revision))) onStep("results");
  }
  async function openDestination() {
    try { await api.openDestination(); } catch (error) { onError(String(error)); }
  }

  const shownDestination = step === "results" && plan ? plan.destination : destination;
  return <>
    <section className="destination-card primary-workspace-card" aria-label="Primary destination"><div className="destination-icon"><FolderOpen size={25} /></div><div className="destination-description"><span className="eyebrow">PRIMARY DESTINATION</span><h2>{shownDestination ? folderName(shownDestination) : "Choose your primary destination"}</h2><p>{shownDestination || "Choose an existing folder, including one already listed as a source, or an empty folder."}</p><small>Existing primary contents stay in place. Primary and overlapping source folders are excluded from incoming mappings.</small></div>{step !== "results" ? <div className="destination-controls"><label><span>Use a listed folder as primary</span><select aria-label="Choose a listed folder as primary destination" value={sourceOptions.some(source => source.input === destination) ? destination : ""} disabled={locked} onChange={event => { if (event.target.value) selectDestination(event.target.value); }}><option value="">Select a listed folder…</option>{sourceOptions.map(source => <option key={source.input} value={source.input}>{source.label} — {source.path}</option>)}</select></label><button className="primary" disabled={locked} onClick={() => void chooseDestination()}>{choosing ? <LoaderCircle size={15} className="spin" /> : <Folder size={15} />}Choose existing folder…</button></div> : <button disabled={locked} onClick={() => onStep("plan")}>Review destination</button>}</section>
    <section className="consolidation" aria-label={step === "results" ? "Consolidation results" : "Merge plan"} hidden={step === "inventory"}>
    {step === "plan" && <>
      <section className="mapping-editor" aria-label="Folder mappings"><div className="section-heading"><div><h2>Choose what belongs here</h2><p>Select source folders and review their destination. Leave the target blank to combine at the primary folder, or enter a subfolder to keep a collection separate.</p></div><button disabled={locked || !inventoryReady} onClick={() => { setMappings([...candidates, { source_key: "", destination_relative: "", selected: false }]); setDirty(true); }}><FolderPlus size={14} /> Add mapping</button></div>
        {!inventoryReady && <p className="plan-empty">{snapshot?.inventory_progress?.state === "finished_with_errors" ? "Resolve the inventory gaps and re-scan before building a merge plan." : "Finish the source inventory to choose folders."} <button onClick={() => onStep("inventory")}>Go to inventory</button></p>}
        {inventoryReady && !folders.length && <p className="plan-empty">No folders are available in the saved inventory.</p>}
        {candidates.map((mapping, index) => <div className={`mapping-row ${mapping.selected ? "included" : ""}`} key={index}><input type="checkbox" aria-label={`Include folder mapping ${index + 1}`} checked={mapping.selected} disabled={locked || primaryRootIds.has(folders.find(entry => entry.key === mapping.source_key)?.root_id ?? -1)} onChange={event => updateMapping(index, { selected: event.target.checked })} /><label><span>Source folder</span><select aria-label={`Source folder for mapping ${index + 1}`} disabled={locked} value={mapping.source_key} onChange={event => updateMapping(index, { source_key: event.target.value })}><option value="">Choose an inventoried folder</option>{folders.map(entry => <option key={entry.key} value={entry.key} disabled={entry.stale || entry.excluded || primaryRootIds.has(entry.root_id)}>{rootName(entry.root_id)}{entry.relative_path ? ` / ${entry.relative_path}` : " (source root)"}{primaryRootIds.has(entry.root_id) ? " — primary / overlapping" : entry.stale ? " — stale" : ""}</option>)}</select><small>{folders.find(entry => entry.key === mapping.source_key) ? sourcePath(folders.find(entry => entry.key === mapping.source_key)!.root_id, folders.find(entry => entry.key === mapping.source_key)!.relative_path) : "Select a folder to review"}</small></label><ArrowRight size={16} /><label><span>Folder within primary destination</span><input value={mapping.destination_relative} disabled={locked} placeholder="Primary folder (combine here)" aria-label={`Destination subfolder for mapping ${index + 1}`} onChange={event => updateMapping(index, { destination_relative: event.target.value })} /><small>{destination ? displayPath(destination, mapping.destination_relative) : "Choose the primary destination above"}</small></label><button className="icon-button" aria-label={`Remove mapping ${index + 1}`} disabled={locked} onClick={() => { setMappings(candidates.filter((_, itemIndex) => itemIndex !== index)); setDirty(true); }}><X size={14} /></button></div>)}
        {invalidRelative && <p className="inline-error">Use a relative folder name without “..” or an absolute path.</p>}
        {invalidSource && <p className="inline-error">Choose a current source folder different from the primary destination.</p>}
        <div className="mapping-actions"><span>{selectedMappings.length} folder mapping{selectedMappings.length === 1 ? "" : "s"} selected{dirty && plan ? " · changes need a new plan" : ""}</span><button className="primary" disabled={locked || !inventoryReady || !destination || !selectedMappings.length || invalidRelative || invalidSource} onClick={() => void buildPlan()}>{snapshot?.worker === "plan" ? <LoaderCircle size={15} className="spin" /> : <LayersIcon />}Build merge plan</button></div>
      </section>
    </>}
    {plan ? <section className="plan-review" aria-label={step === "results" ? "Saved execution results" : "Review planned additions"}>
      <div className="section-heading"><div><h2>{step === "results" ? "Verified results" : "Review every destination"}</h2><p>Revision {plan.revision} · {stateLabels[plan.state]}{step === "plan" ? ". Planned additions have not yet been copied." : ". Results are saved after each verified item."}</p></div><button disabled={busy || snapshot?.worker === "plan"} onClick={() => void openDestination()}><FolderOpen size={14} /> Open destination</button></div>
      {generationChanged && <div className="draft-notice">The source inventory has changed since this plan was built. Build a new merge plan before approving or copying.</div>}
      {!decisionsEditable && step === "plan" && <div className="draft-notice">This plan has already started. Build a new plan to change included items or destination mappings. Its saved results remain available.</div>}
      {dirty && <div className="draft-notice">Destination or mappings have changed. Build a new plan before approving or copying. This table still shows the saved plan for <strong>{plan.destination}</strong>.</div>}
      <PlanSummary plan={plan} results={step === "results"} />
      <div className="plan-list-toolbar"><span>{plan.items.length.toLocaleString()} saved plan items · {selectedItems.length.toLocaleString()} included</span><select aria-label="Filter plan items" value={itemFilter} onChange={event => setItemFilter(event.target.value)}><option value="all">All plan items</option><option value="included">Included items</option><option value="exceptions">Exceptions</option><option value="verified">Verified items</option></select></div>
      <div className="plan-table-scroll"><table className="plan-table" aria-label={step === "results" ? "Item results" : "Planned source and destination paths"}><thead><tr><th>Include</th><th>Source</th><th>Exact destination</th><th>{step === "results" ? "Result" : "Planned action"}</th></tr></thead><tbody>{rows.map(item => <tr key={item.id} className={item.action === "blocked" || item.status === "failed" ? "plan-exception" : ""}><td><input type="checkbox" aria-label={`Include ${sourcePath(item.root_id, item.source_relative)}`} checked={item.included} disabled={locked || step === "results" || needsRebuild || !decisionsEditable || item.status === "verified" || item.action === "blocked"} onChange={event => void act(() => api.decide(plan.revision, [{ item_id: item.id, included: event.target.checked }]))} /></td><td><button className="source-path-link" disabled={!item.source_key} onClick={() => onReveal(item.source_key)}>{sourcePath(item.root_id, item.source_relative)}</button>{item.contributors.length > 1 && <details><summary>{item.contributors.length} contributing placements</summary>{item.contributors.map((contributor, index) => <p key={index}>{sourcePath(contributor.root_id, contributor.source_relative)}</p>)}</details>}</td><td>{displayPath(plan.destination, item.destination_relative)}</td><td><strong>{step === "results" ? item.status === "verified" ? "Verified" : item.status === "failed" ? "Failed — retry needed" : item.action === "blocked" ? "Held — not copied" : !item.included || item.status === "skipped" ? "Excluded" : "Not yet verified" : actionLabels[item.action]}</strong>{step === "results" && <small>{actionLabels[item.action]}</small>}{item.detail && <p>{item.detail}</p>}</td></tr>)}</tbody></table>{!rows.length && <p className="plan-empty">No items match this view.</p>}</div>
      {step === "plan" ? <div className="plan-consent">
        {includedBlocked && <p className="inline-error">Resolve the flagged mappings or exclude blocked items before approval.</p>}
        {!approved && <label><input type="checkbox" checked={consent} disabled={locked || needsRebuild || !decisionsEditable || includedBlocked || !selectedItems.length} onChange={event => setConsent(event.target.checked)} /><span>I reviewed revision {plan.revision} and its exact destination paths. Approve the included additions for copying; retain source files.</span></label>}
        <div><span>{approved ? `Revision ${plan.revision} is approved. Only its included items will run.` : "Approval applies only to this saved revision."}</span>{approved ? <button className="primary" disabled={locked || needsRebuild || includedBlocked || !unfinished.length} onClick={() => void execute()}><Play size={14} /> Copy approved additions</button> : <button className="primary" disabled={locked || needsRebuild || !decisionsEditable || !consent || includedBlocked || !selectedItems.length} onClick={() => void act(() => api.approve(plan.revision))}><Check size={14} /> Approve revision {plan.revision}</button>}</div>
      </div> : <div className="results-actions"><p>{selectedItems.filter(item => item.status === "pending").length.toLocaleString()} items awaiting verification. Source files remain in place.</p><div><button disabled={locked} onClick={() => onStep("plan")}>Review plan</button>{snapshot?.worker === "execute" ? <button disabled={busy || snapshot.pause_requested} onClick={() => void act(api.pause)}><Pause size={14} />{snapshot.pause_requested ? "Saving pause…" : "Pause copying"}</button> : <button className="primary" disabled={locked || needsRebuild || !approved || !unfinished.length || includedBlocked} onClick={() => void execute()}><Play size={14} />{plan.state === "paused" || plan.state === "running" ? "Resume approved additions" : "Retry unfinished additions"}</button>}</div></div>}
    </section> : step === "results" ? <div className="plan-empty"><FolderOpen size={28} /><h2>No copy results yet</h2><p>Build and review a merge plan, then copy its approved additions.</p><button className="primary" onClick={() => onStep("plan")}>Create a merge plan</button></div> : <p className="plan-empty">Choose a destination and select mappings to see exact proposed additions. No files are copied while building a plan.</p>}
  </section></>;
}
function LayersIcon() { return <FolderPlus size={15} />; }
