import { useMemo, useState } from "react";
import { File, FolderOpen } from "lucide-react";
import type { Comparison, Snapshot } from "./api";
export function DuplicateGroups({ groups, roots, onReveal }: { groups: Map<string, Comparison[]>; roots: Snapshot["roots"]; onReveal: (key: string) => void }) {
  const matches = useMemo(() => [...new Set(groups.values())], [groups]);
  const [limit, setLimit] = useState(20);
  return <section className="duplicate-groups" aria-label="Matching file groups"><div className="section-heading"><div><h2>{matches.length.toLocaleString()} matching file groups</h2><p>Matching full-content hashes, grouped across the scanned locations. No copy is chosen for deletion. Overlapping sources may refer to the same physical file.</p></div></div>
    {matches.slice(0, limit).map(group => <details className="duplicate-group" key={`${group[0].algorithm}:${group[0].full_hash}`}><summary><File size={18} /><strong>{group[0].relative_path.split("/").at(-1)}</strong><span>{group.length} locations</span></summary><ul>{group.map(record => <li key={record.key}><FolderOpen size={16} /><span>{roots.find(root => root.id === record.root_id)?.path}/{record.relative_path}</span><button onClick={() => onReveal(record.key)}>Inspect file</button></li>)}</ul></details>)}
    {!matches.length && <p className="plan-empty">No matching groups are available yet. Compare contents to look for identical files; incomplete comparisons may find more as they progress.</p>}
    {matches.length > limit && <button className="preview-more" onClick={() => setLimit(value => value + 20)}>Show next {Math.min(20, matches.length - limit)} groups</button>}
  </section>;
}
