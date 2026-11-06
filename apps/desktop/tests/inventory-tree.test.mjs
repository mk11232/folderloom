import assert from "node:assert/strict";
import test from "node:test";
import { ancestors, buildInventoryTree, entryName, inventoryRows, latestComparisons, matchingContext } from "../src/inventory-tree.ts";

const roots = [1, 2, 3, 4].map(id => ({ id, path: `/synthetic/copy-${id}`, input_path: `/synthetic/copy-${id}` }));
const entry = (key, parent_key, relative_path, entry_type = "directory", root_id = 1) => ({ key, parent_key, relative_path, entry_type, root_id, size: 10, modified_ns: null, link_target: null, excluded: false, stale: false, directory_complete: true });
const entries = [
  entry("opaque-root-a", null, ""),
  entry("not/a/path", "opaque-root-a", "Archive"),
  entry("opaque-subfolder", "not/a/path", "Archive/Notes"),
  entry("opaque-file-a", "opaque-subfolder", "Archive/Notes/report.txt", "file"),
  entry("opaque-file-b", "not/a/path", "Archive/overview.txt", "file"),
  entry("opaque-root-b", null, "", "directory", 2),
  entry("opaque-other-placement", "opaque-root-b", "report.txt", "file", 2),
  entry("opaque-root-c", null, "", "directory", 3),
  entry("opaque-root-d", null, "", "directory", 4),
];

test("four source roots open by default; nested folders collapse without dropping placements", () => {
  const tree = buildInventoryTree(roots, entries);
  const rows = inventoryRows(tree, node => node.source);
  assert.equal(tree.roots.length, 4);
  assert.deepEqual(rows.map(row => [row.node.entry.key, row.depth]), [
    ["opaque-root-a", 0], ["not/a/path", 1], ["opaque-root-b", 0],
    ["opaque-other-placement", 1], ["opaque-root-c", 0], ["opaque-root-d", 0],
  ]);
  assert.equal(inventoryRows(tree, () => true).length, entries.length);
  assert.equal(inventoryRows(tree, () => false).length, 4);
});

test("native parent keys retain distinct placements even when escaped display paths collide", () => {
  const tree = buildInventoryTree(roots.slice(0, 1), [entries[0],
    entry("bytes-ff", "opaque-root-a", "\\xFF"),
    entry("literal-backslash", "opaque-root-a", "\\xFF"),
    entry("child-1", "bytes-ff", "\\xFF/report.txt", "file"),
    entry("child-2", "literal-backslash", "\\xFF/report.txt", "file"),
  ]);
  assert.equal(tree.byEntry.get("child-1").parent.entry.key, "bytes-ff");
  assert.equal(tree.byEntry.get("child-2").parent.entry.key, "literal-backslash");
  assert.equal(entryName(tree.byEntry.get("bytes-ff").entry), "\\xFF");
  assert.equal(inventoryRows(tree, () => true).length, 5);
});

test("filtered matches retain ancestor context, allow collapse, and leave ordinary expansion intact", () => {
  const tree = buildInventoryTree(roots, entries);
  const normalExpansion = new Map([[tree.byEntry.get("not/a/path").id, true]]);
  const normal = node => normalExpansion.get(node.id) ?? node.source;
  const before = inventoryRows(tree, normal);
  const matches = new Set(["opaque-file-a"]);
  const context = matchingContext(tree, matches);
  assert.equal(matches.size, 1);
  assert.deepEqual(inventoryRows(tree, () => true, context).map(row => row.node.entry.key), ["opaque-root-a", "not/a/path", "opaque-subfolder", "opaque-file-a"]);
  const collapsed = new Set([tree.byEntry.get("not/a/path").id]);
  assert.deepEqual(inventoryRows(tree, node => !collapsed.has(node.id), context).map(row => row.node.entry.key), ["opaque-root-a", "not/a/path"]);
  assert.deepEqual(inventoryRows(tree, normal), before);
});

test("source filters preserve hierarchy and placement reveal opens every native ancestor", () => {
  const tree = buildInventoryTree(roots, entries);
  assert.deepEqual(inventoryRows(tree, () => true, undefined, 2).map(row => row.node.entry.key), ["opaque-root-b", "opaque-other-placement"]);
  const target = tree.byEntry.get("opaque-file-a");
  const expanded = new Set(ancestors(target).map(node => node.id));
  const rows = inventoryRows(tree, node => expanded.has(node.id));
  assert.equal(rows.find(row => row.node === target).depth, 3);
});

test("missing parents during an incomplete scan remain visible beneath the correct source", () => {
  const tree = buildInventoryTree(roots, [entry("waiting-child", "not-yet-saved", "pending/report.txt", "file", 3)]);
  const rows = inventoryRows(tree, node => node.source, undefined, 3);
  assert.equal(rows[1].node.entry.key, "waiting-child");
  assert.equal(rows[1].depth, 1);
});

test("comparison filtering uses the newest generation, including stale observations", () => {
  const base = { key: "opaque-file-a", generation: 1, stage: "full_hash", full_hash: "synthetic", stale: false };
  const latest = { ...base, generation: 3, stage: "pending_sample", stale: true };
  const records = latestComparisons([latest, base, { ...base, generation: 2 }]);
  assert.equal(records.get(base.key), latest);
});
