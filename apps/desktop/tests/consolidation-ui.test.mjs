import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import ts from "typescript";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

// Render the actual workspace with synthetic persisted data; native actions are
// not invoked here. Host workflow tests cover native I/O and durable decisions.
const { componentUrl } = await import("./load-component.mjs");
const { Consolidation, PlanSummary, excludePrimaryMappings, pathsOverlap } = await import(await componentUrl("Consolidation"));
const roots = [{ id: 1, path: "/synthetic/Family", input_path: "/synthetic/Family" }, { id: 2, path: "/synthetic/Backup", input_path: "/synthetic/Backup" }];
const snapshot = { job_id: "synthetic", worker: null, roots, entries: roots.map(root => ({ key: `root-${root.id}`, parent_key: null, root_id: root.id, relative_path: "", entry_type: "directory", stale: false, excluded: false })), inventory_progress: { state: "complete", generation: 1 }, plan: null };
const item = (id, action, status = "pending", included = true) => ({ id, root_id: 1, source_key: "root-1", source_relative: `report-${id}.pdf`, destination_relative: `Documents/report-${id}.pdf`, action, status, included, detail: null, contributors: [], hash: null });
const plan = { id: 3, revision: 4, approved_revision: null, destination: "/synthetic/Primary", destination_input: "/synthetic/Primary", generation: 1, state: "proposed", mappings: [{ source_key: "root-1", root_id: 1, source_relative: "", destination_relative: "Documents", destination_input: "Documents" }], items: [item(1, "copy"), item(2, "already_present"), item(3, "create_directory"), { ...item(4, "blocked"), detail: "Source changed; rebuild the plan." }] };
const props = { snapshot, sources: roots.map(root => ({ label: root.path.split("/").at(-1), path: root.path, input_path: root.input_path })), onDestinationChange() {}, busy: false, step: "plan", rootName: id => roots.find(root => root.id === id).path.split("/").at(-1), act: async () => false, onStep() {}, onError() {}, onReveal() {} };
const render = (changes = {}) => renderToStaticMarkup(createElement(Consolidation, { ...props, ...changes }));

test("new mapping candidates are unselected and cannot silently start a copy", () => {
  const html = render();
  assert.match(html, /Include folder Family/);
  assert.match(html, /Include folder Backup/);
  assert.doesNotMatch(html, /type="checkbox"[^>]*checked=""/);
  assert.match(html, /0 folders selected/);
  assert.match(html, /<button class="primary" disabled=""[^>]*>[\s\S]*?Preview destination/);
  assert.doesNotMatch(html, /Copy approved additions/);
});

test("review displays exact source and output paths, actions, and revision-specific consent", () => {
  const html = render({ snapshot: { ...snapshot, plan } });
  assert.match(html, /\/synthetic\/Family\/report-1.pdf/);
  assert.match(html, /\/synthetic\/Primary\/Documents\/report-1.pdf/);
  for (const text of ["Copy to this location", "Already here — no copy", "Needs review", "Approve revision 4", "Source changed; rebuild the plan."]) assert.ok(html.includes(text), text);
  assert.match(html, /Planned additions have not yet been copied/);
  assert.match(html, /Resolve the flagged mappings or exclude blocked items before approval/);
  assert.doesNotMatch(html, /Copy approved additions/);
});

test("results count only verified included work and keep failed and pending items explicit", () => {
  const results = { ...plan, state: "finished_with_errors", approved_revision: 4, items: [item(1, "copy", "verified"), item(2, "already_present", "verified"), item(3, "create_directory", "verified"), { ...item(4, "copy", "failed"), detail: "Read failed." }, item(5, "copy", "pending"), item(6, "copy", "verified", false)] };
  const summary = renderToStaticMarkup(createElement(PlanSummary, { plan: results, results: true }));
  assert.match(summary, /<strong>1<\/strong><span>additions verified/);
  assert.match(summary, /<strong>1<\/strong><span>existing files verified/);
  assert.match(summary, /<strong>1<\/strong><span>exceptions/);
  const html = render({ snapshot: { ...snapshot, plan: results }, step: "results" });
  for (const text of ["Failed — retry needed", "Not yet verified", "Read failed.", "Retry unfinished additions", "Source files remain in place."]) assert.ok(html.includes(text), text);
  assert.match(html, /aria-label="Item results"/);
});

test("editing decisions requires a new approved revision before copying", () => {
  const oldApproval = { ...plan, revision: 5, approved_revision: 4, items: [item(1, "copy")] };
  const html = render({ snapshot: { ...snapshot, plan: oldApproval } });
  assert.match(html, /Approve revision 5/);
  assert.doesNotMatch(html, /Copy approved additions/);
  const approved = render({ snapshot: { ...snapshot, plan: { ...oldApproval, approved_revision: 5 } } });
  assert.match(approved, /Copy approved additions/);
});


test("held items remain visible exceptions even when the plan excludes them", () => {
  const held = { ...plan, items: [item(1, "copy", "verified"), { ...item(2, "blocked", "pending", false), detail: "Link preservation requires review." }, item(3, "copy", "pending", false)] };
  const html = render({ snapshot: { ...snapshot, plan: held }, step: "results" });
  assert.match(html, /<strong>1<\/strong><span>exceptions/);
  assert.match(html, /<strong>1<\/strong><span>excluded items/);
  assert.match(html, /Held — not copied/);
  assert.match(html, /Link preservation requires review/);
  const proposed = renderToStaticMarkup(createElement(PlanSummary, { plan: held }));
  assert.match(proposed, /<strong>1<\/strong><span>need review/);
});


test("inventory gaps and changed generations explain why a plan cannot proceed", () => {
  const gaps = render({ snapshot: { ...snapshot, inventory_progress: { state: "finished_with_errors", generation: 1 } } });
  assert.match(gaps, /Resolve the inventory gaps and re-scan/);
  const stalePlan = render({ snapshot: { ...snapshot, inventory_progress: { state: "complete", generation: 2 }, plan: { ...plan, approved_revision: 4, items: [item(1, "copy")] } } });
  assert.match(stalePlan, /source inventory has changed since this plan was built/);
  assert.match(stalePlan, /<button class="primary" disabled=""[^>]*>[\s\S]*?Copy approved additions/);
});

test("started plans preserve decisions and explain rebuilding", () => {
  const html = render({ snapshot: { ...snapshot, plan: { ...plan, state: "paused", approved_revision: 4, items: [item(1, "copy")] } } });
  assert.match(html, /This plan has already started/);
  assert.match(html, /aria-label="Include \/synthetic\/Family\/report-1.pdf"[^>]*disabled=""/);
});


test("destination controls are available during arrangement and hidden on the scan overview", () => {
  const html = render({ step: "plan" });
  const card = html.slice(html.indexOf("primary-workspace-card"), html.indexOf("class=\"consolidation\""));
  assert.match(card, /aria-label="Primary destination"/);
  assert.doesNotMatch(card, /hidden=/);
  assert.match(card, /Choose existing folder…/);
  assert.match(card, /Choose an existing or empty folder/);
  assert.match(card, /aria-label="Choose a listed folder as primary destination"/);
  assert.match(card, /value="\/synthetic\/Family"/);
  assert.match(card, /value="\/synthetic\/Backup"/);
  assert.match(card, /Existing primary contents stay in place/);
  assert.match(render({ step: "inventory" }), /aria-label="Primary destination" hidden=""/);
});

test("primary selection excludes overlapping incoming mappings without confusing sibling paths", () => {
  const incoming = [{ source_key: "family", selected: true }, { source_key: "archive", selected: true }, { source_key: "sibling", selected: true }].map(mapping => ({ ...mapping, destination_relative: "" }));
  const entries = [{ key: "family", root_id: 1 }, { key: "archive", root_id: 2 }, { key: "sibling", root_id: 3 }];
  const sourceRoots = [{ id: 1, input_path: "/synthetic/Family" }, { id: 2, input_path: "/synthetic/Family/Archive" }, { id: 3, input_path: "/synthetic/Family-old" }];
  assert.deepEqual(excludePrimaryMappings(incoming, entries, sourceRoots, "/synthetic/Family/").map(mapping => mapping.selected), [false, false, true]);
  assert.deepEqual(excludePrimaryMappings(incoming, entries, sourceRoots, "/synthetic/Family/Archive/Primary").map(mapping => mapping.selected), [false, false, true]);
  assert.equal(pathsOverlap("/synthetic/folder\\name", "/synthetic/folder"), false);
  assert.equal(incoming[0].selected, true, "exclusion must not mutate the previous draft");
});


test("folder arrangement offers concrete output choices and keeps custom mappings secondary", () => {
  const html = render();
  for (const text of ["Combine contents", "Keep each folder separate", "Folder arrangement", "Folders to include", "WHERE FILES WILL GO", "Preview destination"]) assert.ok(html.includes(text), text);
  assert.doesNotMatch(html, /Source folder for mapping/);
});

test("saved review uses destination folders instead of a source-destination table", () => {
  const html = render({ snapshot: { ...snapshot, plan } });
  assert.match(html, /Destination preview/);
  assert.match(html, /class="file-outcome/);
  assert.match(html, /Paths below are inside the destination/);
  assert.match(html, /type="checkbox" aria-label="Include/);
  assert.doesNotMatch(html, /<table/);
  assert.match(html, /Needs attention \(1\)/);
});
