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
  onDecision: (item: PlanItem, included: boolean) => void;
};
const labels = { copy: "Copy addition", already_present: "Already present", create_directory: "Create folder", blocked: "Needs review" };
function outcome(item: PlanItem, results: boolean) {
  if (!results) return labels[item.action];
  return item.status === "verified" ? "Verified" : item.status === "failed" ? "Failed — retry needed" : item.action === "blocked" ? "Held — not copied" : !item.included || item.status === "skipped" ? "Excluded" : "Not yet verified";
}
function ItemDetails({ item, ...props }: Props & { item: PlanItem }) {
  const source = props.sourcePath(item.root_id, item.source_relative);
  const destination = `${props.plan.destination.replace(/\/$/, "")}${item.destination_relative ? `/${item.destination_relative}` : ""}`;
  return <details className={`preview-file ${item.action === "blocked" || item.status === "failed" ? "needs-attention" : ""}`}>
    <summary>{item.action === "create_directory" ? <Folder size={16} /> : <File size={16} />}<span>{item.destination_relative.split("/").at(-1) || "Destination folder"}</span><small>{!item.included && item.action !== "blocked" ? "Left out" : outcome(item, props.results)}</small></summary>
    <div className="preview-file-details">
      <dl><dt>From</dt><dd><button className="source-path-link" disabled={!item.source_key} onClick={() => props.onReveal(item.source_key)}>{source}</button></dd><dt>Exact destination</dt><dd>{destination}</dd><dt>{props.results ? "Result" : "Planned action"}</dt><dd>{outcome(item, props.results)}{props.results && ` · ${labels[item.action]}`}</dd></dl>
      {item.detail && <p>{item.detail}</p>}
      {item.contributors.length > 1 && <details><summary>{item.contributors.length} contributing placements</summary>{item.contributors.map((contributor, index) => <p key={index}>{props.sourcePath(contributor.root_id, contributor.source_relative)}</p>)}</details>}
      {!props.results && <label className="include-file"><input type="checkbox" aria-label={`Include ${source}`} checked={item.included} disabled={props.locked || !props.editable || item.status === "verified" || item.action === "blocked"} onChange={event => props.onDecision(item, event.target.checked)} />Include this item in the copy</label>}
    </div>
  </details>;
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
export function PlanPreview(props: Props) {
  const [filter, setFilter] = useState("all");
  const [search, setSearch] = useState("");
  const [limit, setLimit] = useState(50);
  const attention = props.plan.items.filter(item => item.action === "blocked" || item.status === "failed").length;
  const items = useMemo(() => props.plan.items.filter(item => {
    if (filter === "attention" && item.action !== "blocked" && item.status !== "failed") return false;
    if (filter === "additions" && (item.action !== "copy" || !item.included)) return false;
    if (filter === "existing" && item.action !== "already_present") return false;
    if (filter === "excluded" && item.included) return false;
    const query = search.trim().toLocaleLowerCase();
    return !query || `${item.destination_relative} ${props.sourcePath(item.root_id, item.source_relative)}`.toLocaleLowerCase().includes(query);
  }), [props.plan.items, filter, search, props.sourcePath]);
  const tree = useMemo(() => destinationTree(items), [items]);
  return <section className="destination-preview" aria-label={props.results ? "Item results" : "Planned source and destination paths"}>
    <div className="preview-controls"><label>Find a file<input type="search" value={search} onChange={event => { setSearch(event.target.value); setLimit(50); }} placeholder="Search source or destination…" /></label>
      <label>Show<select aria-label="Filter plan items" value={filter} onChange={event => { setFilter(event.target.value); setLimit(50); }}><option value="all">Everything</option><option value="attention">Needs attention ({attention})</option><option value="additions">Files to add</option><option value="existing">Already present</option><option value="excluded">Left out</option></select></label>
    </div>
    {attention > 0 && <button className="attention-shortcut" onClick={() => { setFilter("attention"); setLimit(50); }}>{attention} items need attention — inspect them</button>}
    <div className="preview-root"><Folder size={20} /><strong>{props.plan.destination.split("/").at(-1) || props.plan.destination}</strong><span>{items.length.toLocaleString()} of {props.plan.items.length.toLocaleString()} items</span></div>
    <p className="preview-help">This is the destination layout. Open a folder, then a file to see its source{props.results ? " and saved result." : " or leave it out."}</p>
    <div className="preview-tree" key={`${props.plan.id}:${filter}:${search}`}>
      {tree.items.map(item => <ItemDetails key={item.id} item={item} {...props} />)}
      {tree.children.slice(0, limit).map(node => <Branch key={node.path} node={node} depth={0} {...props} />)}
      {tree.children.length > limit && <button className="preview-more" onClick={() => setLimit(previous => previous + 50)}>Show next {Math.min(50, tree.children.length - limit)} items</button>}
      {!items.length && <p className="plan-empty">No items match this view.</p>}
    </div>
  </section>;
}
