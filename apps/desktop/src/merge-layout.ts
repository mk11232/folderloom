import type { Entry, MappingRequest, MergePlan, PlanItem, Snapshot } from "./api.ts";

export type DraftMapping = MappingRequest & { selected: boolean };
export type MergeLayout = "combine" | "separate" | "custom";

// These are new, explicit output names, never identities reconstructed from display paths.
export function separateFolderNames(roots: Snapshot["roots"]) {
  const used = new Set<string>();
  const names = new Map<number, string>();
  for (const root of roots) {
    const base = (root.input_path ?? root.path).replace(/\/+$/, "").split("/").at(-1)?.replace(/[\\:\0]/g, "-") || `Source ${root.id}`;
    const safe = base === "." || base === ".." ? `Source ${root.id}` : base;
    let name = safe;
    let suffix = 2;
    while (used.has(name.normalize("NFC").toLowerCase())) name = `${safe} (${suffix++})`;
    used.add(name.normalize("NFC").toLowerCase());
    names.set(root.id, name);
  }
  return names;
}
export function layoutMappings(layout: Exclude<MergeLayout, "custom">, entries: Entry[], roots: Snapshot["roots"], previous: DraftMapping[]): DraftMapping[] {
  const names = separateFolderNames(roots);
  const selected = new Set(previous.filter(item => item.selected).map(item => item.source_key));
  return entries.filter(entry => entry.entry_type === "directory" && entry.parent_key === null).map(entry => ({
    source_key: entry.key,
    destination_relative: layout === "combine" ? "" : names.get(entry.root_id) ?? `Source ${entry.root_id}`,
    selected: selected.has(entry.key) && !entry.stale && !entry.excluded,
  }));
}
export function savedLayout(plan: MergePlan | null | undefined, entries: Entry[], roots: Snapshot["roots"]): MergeLayout {
  if (!plan) return "combine";
  const rootKeys = new Map(entries.filter(entry => entry.parent_key === null && entry.entry_type === "directory").map(entry => [entry.key, entry.root_id]));
  if (plan.mappings.some(mapping => !rootKeys.has(mapping.source_key) || mapping.destination_input === null)) return "custom";
  if (plan.mappings.every(mapping => mapping.destination_input === "")) return "combine";
  const names = separateFolderNames(roots);
  return plan.mappings.every(mapping => mapping.destination_input === names.get(rootKeys.get(mapping.source_key)!)) ? "separate" : "custom";
}

export type PreviewNode = { path: string; name: string; children: PreviewNode[]; items: PlanItem[]; count: number };
export function destinationTree(items: PlanItem[]): PreviewNode {
  const root: PreviewNode = { path: "", name: "", children: [], items: [], count: 0 };
  const children = new Map<PreviewNode, Map<string, PreviewNode>>();
  for (const item of items) {
    let node = root;
    node.count++;
    for (const part of item.destination_relative.split("/").filter(Boolean)) {
      let indexed = children.get(node);
      if (!indexed) { indexed = new Map(); children.set(node, indexed); }
      let child = indexed.get(part);
      if (!child) {
        child = { path: node.path ? `${node.path}/${part}` : part, name: part, children: [], items: [], count: 0 };
        indexed.set(part, child); node.children.push(child);
      }
      node = child; node.count++;
    }
    node.items.push(item);
  }
  const sort = (node: PreviewNode) => {
    node.children.sort((a, b) => Number(b.children.length > 0 || b.items.some(item => item.action === "create_directory")) - Number(a.children.length > 0 || a.items.some(item => item.action === "create_directory")) || a.name.localeCompare(b.name));
    node.children.forEach(sort);
  };
  sort(root);
  return root;
}
