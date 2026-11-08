import { useMemo, useState } from "react";
import { File, Folder } from "lucide-react";
import type { MergePlan, PlanItem } from "./api";
import { destinationTree, type PreviewNode } from "./merge-layout";

type Props = {
  plan: MergePlan;
  results: boolean;
  locked: boolean;
  editable: boolean;
  sourcePath: (root: number, relative: string) => string;
  onReveal: (key: string) => void;
  evidence?: Map<number, { same: string[]; different: string[] }>;
  onDecision: (item: PlanItem, included: boolean) => void;
};
const labels = { copy: "Copy to this location", already_present: "Already here — no copy", create_directory: "Keep folder", blocked: "Needs review" };
export function comparePlanItems(items: PlanItem[]) {
  const hashes = new Map<string, PlanItem[]>();
  const names = new Map<string, Map<string, PlanItem[]>>();
  for (const item of items) {
    if (!item.hash || item.action === "create_directory" || item.action === "blocked") continue;
    const name = item.source_relative.split("/").at(-1) ?? "";
    if (!hashes.has(item.hash)) hashes.set(item.hash, []);
    hashes.get(item.hash)!.push(item);
    if (!names.has(name)) names.set(name, new Map());
    const versions = names.get(name)!;
    if (!versions.has(item.hash)) versions.set(item.hash, []);
    versions.get(item.hash)!.push(item);
  }
  return new Map(items.map(item => [item.id, {
    same: [...new Set((item.hash ? hashes.get(item.hash) ?? [] : []).slice(0, 6).filter(other => other.id !== item.id).slice(0, 5).map(other => other.destination_relative))],
    different: [...new Set((item.hash ? [...(names.get(item.source_relative.split("/").at(-1) ?? "") ?? new Map<string, PlanItem[]>()).entries()] : []).filter(([hash]) => hash !== item.hash).slice(0, 5).map(([, group]) => group[0].destination_relative))],
  }]));
}
// Relationships use item IDs: display paths can collide for non-text native paths.
export function relatedFileIds(items: PlanItem[], selectedId: number) {
  const selected = items.find(item => item.id === selectedId);
  if (!selected || !selected.hash || selected.action === "blocked" || selected.action === "create_directory") return new Set<number>();
  const name = selected.source_relative.split("/").at(-1);
  return new Set(items.filter(item => item.action !== "create_directory" && item.action !== "blocked" && item.hash && (item.hash === selected.hash || item.source_relative.split("/").at(-1) === name)).map(item => item.id));
}
export function plannedOutcome(item: PlanItem) {
  if (item.action === "blocked") return "Needs review — no copy";
  if (item.action === "already_present") return item.included ? "Already here — verify only" : "Already here — unchanged";
  if (!item.included) return "Will not copy";
  return labels[item.action];
}
function outcome(item: PlanItem, results: boolean) {
  if (!results) return plannedOutcome(item);
  return item.status === "verified" ? "Verified" : item.status === "failed" ? "Failed — retry needed" : item.action === "blocked" ? "Held — not copied" : !item.included || item.status === "skipped" ? "Excluded" : "Not yet verified";
}
function ItemDetails({ item, ...props }: Props & { item: PlanItem }) {
  const source = props.sourcePath(item.root_id, item.source_relative);
  const destination = `${props.plan.destination.replace(/\/$/, "")}${item.destination_relative ? `/${item.destination_relative}` : ""}`;
  const folder = item.action === "create_directory";
  const evidence = props.evidence?.get(item.id);
  const contributors = item.contributors.length ? item.contributors : [{ root_id: item.root_id, source_relative: item.source_relative }];
  const sourceLabel = (root: number, path: string) => `${props.sourcePath(root, "").replace(/\/$/, "").split("/").at(-1)} / ${path}`;
  return <div className={`file-outcome ${item.action === "blocked" || item.status === "failed" ? "needs-attention" : ""}`}>
    <div className="file-outcome-main">
      {!props.results && <input type="checkbox" aria-label={`Include ${source}`} title={folder ? "Include this folder only; files inside are selected separately" : `Include ${item.destination_relative}`} checked={item.included} disabled={props.locked || !props.editable || item.status === "verified" || item.action === "blocked"} onChange={event => props.onDecision(item, event.target.checked)} />}
      {folder ? <Folder size={16} /> : <File size={16} />}<strong title={destination}>{item.destination_relative || "Destination folder"}</strong>
      <span>{!item.included && item.action !== "blocked" ? "Left out" : outcome(item, props.results)}</span>
    </div>
    <div className="file-outcome-info">
      <span className="file-sources">{contributors.map((c, i) => <span key={i} title={props.sourcePath(c.root_id, c.source_relative)}>{sourceLabel(c.root_id, c.source_relative)}{i < contributors.length - 1 ? " · " : ""}</span>)}</span>
      {!folder && item.hash && contributors.length > 1 && <span>{contributors.length} identical source copies → one destination location</span>}
      {!!evidence?.same.length && <span className="same-content">Same contents elsewhere (examples): {evidence.same.join(" · ")} — separate locations retained</span>}
      {!!evidence?.different.length && <span className="different-content">Same name, different contents (examples): {evidence.different.join(" · ")}</span>}
      {folder && !props.results && <small>Folder only; files inside are selected separately.</small>}
      {(item.action === "blocked" || item.status === "failed") && item.detail && <p>{item.detail}</p>}
      <button className="source-path-link" disabled={!item.source_key} onClick={() => props.onReveal(item.source_key)}>Locate source</button>
    </div>
  </div>;
}
function Branch({ node, depth, ...props }: Props & { node: PreviewNode; depth: number }) {
  const [expanded, setExpanded] = useState(depth === 0);
  const [limit, setLimit] = useState(50);
  const folder = node.children.length > 0 || node.items.some(item => item.action === "create_directory");
  if (!folder) return <>{node.items.map(item => <ItemDetails key={item.id} item={item} {...props} />)}</>;
  return <details className="preview-folder" open={expanded} onToggle={event => setExpanded(event.currentTarget.open)}>
    <summary><Folder size={18} /><span>{node.name}</span><small>{node.count.toLocaleString()} items{node.items[0] ? ` · ${outcome(node.items[0], props.results)}` : ""}</small></summary>
    {expanded && <div className="preview-children">
      {node.items.map(item => <ItemDetails key={item.id} item={item} {...props} />)}
      {node.children.slice(0, limit).map(child => <Branch key={child.path} node={child} depth={depth + 1} {...props} />)}
      {node.children.length > limit && <button className="preview-more" onClick={() => setLimit(previous => previous + 50)}>Show next {Math.min(50, node.children.length - limit)} items</button>}
    </div>}
  </details>;
}
export function PlanPreview(input: Props) {
  const evidence = useMemo(() => comparePlanItems(input.plan.items), [input.plan.items]);
  const props = { ...input, evidence };
  const [view, setView] = useState("files");
  const [comparing, setComparing] = useState<number | null>(null);
  const [sort, setSort] = useState("path");
  const comparedFile = props.plan.items.find(item => item.id === comparing);
  const relatedIds = useMemo(() => comparing === null ? null : relatedFileIds(props.plan.items, comparing), [props.plan.items, comparing]);
  const [filter, setFilter] = useState("all");
  const [search, setSearch] = useState("");
  const [limit, setLimit] = useState(50);
  const attention = props.plan.items.filter(item => item.action === "blocked" || item.status === "failed").length;
  const items = useMemo(() => props.plan.items.filter(item => {
    if (view === "files" && item.action === "create_directory") return false;
    if (relatedIds && !relatedIds.has(item.id)) return false;
    if (filter === "attention" && item.action !== "blocked" && item.status !== "failed") return false;
    if (filter === "additions" && (item.action !== "copy" || !item.included)) return false;
    if (filter === "existing" && item.action !== "already_present") return false;
    if (filter === "excluded" && item.included) return false;
    const query = search.trim().toLocaleLowerCase();
    return !query || `${item.destination_relative} ${props.sourcePath(item.root_id, item.source_relative)}`.toLocaleLowerCase().includes(query);
  }).sort((a, b) => {
    if (comparing !== null && (a.id === comparing || b.id === comparing)) return a.id === comparing ? -1 : 1;
    const order = sort === "outcome" ? outcome(a, props.results).localeCompare(outcome(b, props.results)) : 0;
    return order || a.destination_relative.localeCompare(b.destination_relative) || a.id - b.id;
  }), [props.plan.items, filter, search, props.sourcePath, view, relatedIds, comparing, sort, props.results]);
  const tree = useMemo(() => destinationTree(items), [items]);
  return <section className="destination-preview" aria-label={props.results ? "Item results" : "Planned source and destination paths"}>
    <div className="preview-toolbar"><div className="preview-view" role="group" aria-label="Preview view"><button aria-pressed={view === "files"} onClick={() => setView("files")}>Files</button><button aria-pressed={view === "folders"} onClick={() => { setView("folders"); setComparing(null); }}>Folders</button></div>
    <div className="preview-controls"><label>Find a file<input type="search" value={search} onChange={event => { setSearch(event.target.value); setLimit(50); }} placeholder="Find a file or folder…" disabled={comparing !== null} /></label>
      <label>Show<select aria-label="Filter plan items" disabled={comparing !== null} value={filter} onChange={event => { setFilter(event.target.value); setLimit(50); }}><option value="all">Everything</option><option value="attention">Needs attention ({attention})</option><option value="additions">Files to add</option><option value="existing">Already present</option><option value="excluded">Left out</option></select></label>
      {view === "files" && <label>Sort<select aria-label="Sort files" value={sort} onChange={event => { setSort(event.target.value); setLimit(50); }}><option value="path">File location</option><option value="outcome">What happens</option></select></label>}
    </div></div>
    {comparedFile && <div className="comparison-context"><span>Compared with <strong>{comparedFile.destination_relative}</strong></span><button onClick={() => { setComparing(null); setLimit(50); }}>Show all files</button></div>}
    {attention > 0 && <button className="attention-shortcut" onClick={() => { setComparing(null); setSearch(""); setFilter("attention"); setLimit(50); }}>{attention} items need attention — inspect them</button>}
    <div className="preview-root"><span>{items.length.toLocaleString()} {view === "files" ? "files" : "items"}{comparedFile ? " in this comparison" : " in this view"}</span></div>
    <p className="preview-help">{props.results ? "Saved results for this job." : "Paths are inside the destination. Same contents at different paths stay separate."} Existing destination files outside this plan are not listed.</p>
    <div className="preview-tree" key={`${props.plan.id}:${filter}:${search}`}>
      {view === "files" ? <div className="file-table-scroll"><table className="file-outcomes-table" aria-label="File destinations and content matches"><thead><tr><th>Include</th><th>File / destination location</th><th>Sources</th><th>Content match</th><th>What happens</th></tr></thead><tbody>{items.slice(0, limit).map(item => {
        const relationships = evidence.get(item.id);
        const contributors = item.contributors.length ? item.contributors : [{root_id: item.root_id, source_relative: item.source_relative}];

        return <tr key={item.id} className={comparing === item.id ? "comparison-anchor" : undefined}><td>{!props.results ? <input type="checkbox" aria-label={`Include ${props.sourcePath(item.root_id, item.source_relative)}`} checked={item.included} disabled={props.locked || !props.editable || item.status === "verified" || item.action === "blocked"} onChange={event => props.onDecision(item, event.target.checked)} /> : item.status === "verified" ? "✓" : "—"}</td><td><span title={`${props.plan.destination}/${item.destination_relative}`}>{item.destination_relative}</span>{(item.action === "blocked" || item.status === "failed") && item.detail && <small className="file-error">{item.detail}</small>}</td><td>{contributors.map((c, index) => <span key={index}>{index > 0 ? ", " : ""}<span title={props.sourcePath(c.root_id, c.source_relative)}>{props.sourcePath(c.root_id, "").replace(/\/$/, "").split("/").at(-1)}{c.source_relative !== item.destination_relative && <small> / {c.source_relative}</small>}</span></span>)}</td><td>{comparedFile ? item.id === comparedFile.id ? "Selected file" : item.hash === comparedFile.hash ? "Same contents" : "Different contents" : <>{item.hash && contributors.length > 1 && <div>Listed sources identical</div>}{!!relationships?.same.length && <div>Identical file elsewhere</div>}{!!relationships?.different.length && <div>Same name, different contents</div>}{!item.hash ? "Not established" : contributors.length <= 1 && !relationships?.same.length && !relationships?.different.length ? "No match in this plan" : null}{!!(relationships?.same.length || relationships?.different.length) && <button aria-label={`Compare related files for ${item.destination_relative}`} onClick={() => { setComparing(item.id); setSearch(""); setFilter("all"); setLimit(50); }}>Compare</button>}</>}</td><td>{props.results ? outcome(item, true) : plannedOutcome(item)}</td></tr>;
      })}</tbody></table></div> : <>
      {tree.items.map(item => <ItemDetails key={item.id} item={item} {...props} />)}
      {tree.children.slice(0, limit).map(node => <Branch key={node.path} node={node} depth={0} {...props} />)}
      {tree.children.length > limit && <button className="preview-more" onClick={() => setLimit(previous => previous + 50)}>Show next {Math.min(50, tree.children.length - limit)} items</button>}
      </>}
      {view === "files" && items.length > limit && <button className="preview-more" onClick={() => setLimit(previous => previous + 50)}>Show next {Math.min(50, items.length - limit)} files</button>}
      {!items.length && <p className="plan-empty">No items match this view.</p>}
    </div>
  </section>;
}
