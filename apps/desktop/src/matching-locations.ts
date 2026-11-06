import type { Comparison, Entry } from "./api.ts";
import { latestComparisons } from "./inventory-tree.ts";

// Resolve against the whole saved inventory, before presentation filters. A location
// is a saved placement; overlapping selected roots may reference the same file.
export function fullHashLocations(entries: Entry[], comparisons: Comparison[]) {
  const currentFiles = new Set(entries.filter(entry => entry.entry_type === "file" && !entry.stale && !entry.excluded).map(entry => entry.key));
  const groups = new Map<string, Comparison[]>();
  for (const record of latestComparisons(comparisons).values()) {
    if (!currentFiles.has(record.key) || record.stale || record.stage !== "full_hash" || !record.full_hash) continue;
    const content = `${record.algorithm}:${record.full_hash}`;
    const group = groups.get(content) ?? [];
    group.push(record);
    groups.set(content, group);
  }
  const locations = new Map<string, Comparison[]>();
  for (const group of groups.values()) {
    if (group.length < 2) continue;
    for (const record of group) locations.set(record.key, group);
  }
  return locations;
}
