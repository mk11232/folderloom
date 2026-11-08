import assert from "node:assert/strict";
import test from "node:test";
import { destinationTree, layoutMappings, savedLayout, separateFolderNames } from "../src/merge-layout.ts";
const roots = [{ id: 1, path: "/disk-a/Case", input_path: "/disk-a/Case" }, { id: 2, path: "/disk-b/case", input_path: "/disk-b/case" }, { id: 3, path: "/disk-c/Case (2)", input_path: "/disk-c/Case (2)" }];
const entries = roots.map(root => ({ key: `root-${root.id}`, root_id: root.id, parent_key: null, entry_type: "directory", stale: false, excluded: false }));

test("separate layout disambiguates colliding folder names without flattening nested paths", () => {
  assert.deepEqual([...separateFolderNames(roots).values()], ["Case", "case (2)", "Case (2) (2)"]);
  const combined = layoutMappings("combine", entries, roots, [{ source_key: "root-1", selected: true }]);
  assert.deepEqual(combined.map(item => item.destination_relative), ["", "", ""]);
  const separate = layoutMappings("separate", entries, roots, combined);
  assert.deepEqual(separate.map(item => item.selected), [true, false, false]);
  assert.deepEqual(separate.map(item => item.destination_relative), ["Case", "case (2)", "Case (2) (2)"]);
});

test("switching from a specific subfolder never silently includes its entire source root", () => {
  const mappings = layoutMappings("combine", [...entries, { ...entries[0], key: "nested", parent_key: "root-1" }], roots, [{ source_key: "nested", selected: true }]);
  assert.ok(mappings.every(mapping => !mapping.selected));
  const stale = layoutMappings("combine", [{ ...entries[0], stale: true }], roots, [{ source_key: "root-1", selected: true }]);
  assert.equal(stale[0].selected, false);
});

test("saved custom destinations remain custom on reopen", () => {
  assert.equal(savedLayout({ mappings: [{ source_key: "root-1", destination_input: "Documents" }] }, entries, roots), "custom");
  assert.equal(savedLayout({ mappings: [{ source_key: "root-1", destination_input: null }] }, entries, roots), "custom");
  assert.equal(savedLayout({ mappings: [{ source_key: "root-1", destination_input: "" }] }, entries, roots), "combine");
  assert.equal(savedLayout({ mappings: [{ source_key: "root-1", destination_input: "Case" }, { source_key: "root-2", destination_input: "case (2)" }] }, entries, roots), "separate");
});

test("preview preserves multiple items at one displayed path and root directory items", () => {
  const items = [{ id: 1, destination_relative: "", action: "create_directory" }, { id: 2, destination_relative: "Docs/report.pdf", action: "copy" }, { id: 3, destination_relative: "Docs/report.pdf", action: "blocked" }, { id: 4, destination_relative: "Docs/report (conflict).pdf", action: "copy" }];
  const tree = destinationTree(items);
  assert.equal(tree.count, 4);
  assert.equal(tree.items[0].id, 1);
  assert.equal(tree.children[0].count, 3);
  assert.deepEqual(tree.children[0].children.find(node => node.name === "report.pdf").items.map(item => item.id), [2, 3]);
});
