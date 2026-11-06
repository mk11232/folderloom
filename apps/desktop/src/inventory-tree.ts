import type { Comparison, Entry, Snapshot } from "./api.ts";

export type InventoryNode = {
  id: string;
  rootId: number;
  source: boolean;
  entry?: Entry;
  parent: InventoryNode | null;
  children: InventoryNode[];
};
export type InventoryTree = { roots: InventoryNode[]; byEntry: Map<string, InventoryNode>; folders: InventoryNode[] };

// Display text is never used as identity or to reconstruct a parent path.
export function entryName(entry: Entry) { return entry.relative_path.split("/").at(-1) || "Source root"; }

export function latestComparisons(comparisons: Comparison[]) {
  const records = new Map<string, Comparison>();
  for (const record of comparisons) {
    const previous = records.get(record.key);
    if (!previous || record.generation > previous.generation) records.set(record.key, record);
  }
  return records;
}

export function buildInventoryTree(roots: Snapshot["roots"], entries: Entry[]): InventoryTree {
  const sourceNodes = roots.map(root => ({ id: `source:${root.id}`, rootId: root.id, source: true, parent: null, children: [] } as InventoryNode));
  const sources = new Map(sourceNodes.map(node => [node.rootId, node]));
  const byEntry = new Map<string, InventoryNode>();
  for (const entry of entries) {
    const source = sources.get(entry.root_id);
    if (entry.parent_key === null && source && !source.entry) {
      source.entry = entry;
      byEntry.set(entry.key, source);
    } else {
      byEntry.set(entry.key, { id: `entry:${entry.key}`, rootId: entry.root_id, source: false, entry, parent: null, children: [] });
    }
  }
  for (const node of byEntry.values()) {
    if (node.source) continue;
    const parent = node.entry?.parent_key ? byEntry.get(node.entry.parent_key) : undefined;
    // A temporarily missing parent during a scan must not hide a saved placement.
    node.parent = parent && parent.rootId === node.rootId && parent.entry?.entry_type === "directory" ? parent : sources.get(node.rootId) ?? null;
    node.parent?.children.push(node);
  }
  const sort = (nodes: InventoryNode[]) => {
    nodes.sort((a, b) => Number(b.entry?.entry_type === "directory") - Number(a.entry?.entry_type === "directory") || entryName(a.entry!).localeCompare(entryName(b.entry!), undefined, { numeric: true }) || a.id.localeCompare(b.id));
    for (const node of nodes) sort(node.children);
  };
  for (const source of sourceNodes) sort(source.children);
  return { roots: sourceNodes, byEntry, folders: [...sourceNodes, ...[...byEntry.values()].filter(node => !node.source && node.entry?.entry_type === "directory")] };
}

export function ancestors(node: InventoryNode) {
  const result: InventoryNode[] = [];
  for (let parent = node.parent; parent; parent = parent.parent) result.push(parent);
  return result;
}

// Include ancestor context separately from the count of actual matching entries.
export function matchingContext(tree: InventoryTree, matches: Set<string>) {
  const included = new Set<string>();
  for (const key of matches) {
    const node = tree.byEntry.get(key);
    if (node) for (const item of [node, ...ancestors(node)]) included.add(item.id);
  }
  return included;
}

export function inventoryRows(tree: InventoryTree, expanded: (node: InventoryNode) => boolean, included?: Set<string>, rootId: number | null = null) {
  const rows: { node: InventoryNode; depth: number }[] = [];
  const visit = (node: InventoryNode, depth: number) => {
    if (included && !included.has(node.id)) return;
    rows.push({ node, depth });
    if (expanded(node)) for (const child of node.children) visit(child, depth + 1);
  };
  for (const root of tree.roots) if (rootId === null || root.rootId === rootId) visit(root, 0);
  return rows;
}
