import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import ts from "typescript";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createDraftWriter, incomingSources, jobStatus, resumeStep, setupProblem } from "../src/job-workflow.ts";

async function component(name) {
  const source = await readFile(new URL(`../src/${name}.tsx`, import.meta.url), "utf8");
  let compiled = ts.transpileModule(source, { compilerOptions: { jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
  compiled = compiled.replace(/from "([^"]+)"/g, (_, module) => {
    if (module === "./api") return 'from "data:text/javascript,export const api = {}"';
    if (module === "@tauri-apps/plugin-dialog") return 'from "data:text/javascript,export const open = () => Promise.resolve(null)"';
    return `from ${JSON.stringify(module === "./job-workflow" ? new URL("../src/job-workflow.ts", import.meta.url).href : import.meta.resolve(module))}`;
  });
  return (await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`))[name];
}
const JobsHome = await component("JobsHome");
const JobWizard = await component("JobWizard");
const setup = { name: "Family collection", sources: [{ label: "Laptop", path: "/synthetic/Laptop" }, { label: "Backup", path: "/synthetic/Backup" }], destination: "/synthetic/Primary" };
const summary = { id: "a", name: "Family collection", created_ms: 1000, updated_ms: 2000, source_count: 2, entry_count: 17, state: "complete", stage: "inventory", destination: "/synthetic/Primary", error: null, active: false };

test("home table exposes saved jobs and distinguishes inventory from copied results", () => {
  const html = renderToStaticMarkup(createElement(JobsHome, { jobs: [summary, { ...summary, id: "b", name: "Finished archive", stage: "results" }], loading: false, busy: false, workingJob: null, error: null, hasDraft: true, onNew() {}, onDraft() {}, onOpen() {}, onRefresh() {} }));
  for (const text of ["Saved jobs", "Family collection", "Inventory ready", "Copy results ready", "Continue setup", "Discard setup", "New job", "Open Family collection", "/synthetic/Primary"]) assert.ok(html.includes(text), text);
  assert.doesNotMatch(html, /Sign in|Log in|Scan selected folders/);
});

test("sources step offers explicit removal from a saved setup without deleting files", () => {
  const html = renderToStaticMarkup(createElement(JobWizard, { initial: setup, busy: false, onCancel() {}, onStart: async () => true }));
  for (const text of ["Job name", "Family collection", "Remove source Laptop", "Remove source Backup", "never deletes a folder or its files", "Next: destination", "Setup saved locally", "Cancel setup", "Save &amp; close"]) assert.ok(html.includes(text), text);
  assert.doesNotMatch(html, /Copy approved additions/);
});

test("draft writes serialize a removal behind an earlier slow save", async () => {
  const written = [];
  let release;
  const held = new Promise(resolve => { release = resolve; });
  const writer = createDraftWriter(async value => { if (!written.length) await held; written.push(value); });
  const first = writer.save(setup);
  const removed = { ...setup, sources: setup.sources.slice(1) };
  const second = writer.save(removed);
  await Promise.resolve();
  assert.equal(written.length, 0);
  release(); await Promise.all([first, second]); await writer.flush();
  assert.deepEqual(written.map(value => value.sources.map(source => source.label)), [["Laptop", "Backup"], ["Backup"]]);
  assert.equal(setup.sources.length, 2);
});

test("a failed draft save is surfaced and can be retried without restoring removed sources", async () => {
  let attempts = 0;
  let persisted;
  const writer = createDraftWriter(async value => { if (++attempts === 1) throw new Error("disk unavailable"); persisted = value; });
  await assert.rejects(writer.save(setup), /disk unavailable/);
  await assert.rejects(writer.flush(), /disk unavailable/);
  const removed = { ...setup, sources: setup.sources.slice(1) };
  await writer.save(removed); await writer.flush();
  assert.deepEqual(persisted.sources, [setup.sources[1]]);
});

test("primary can be a listed source, but ancestor overlap and empty incoming sets block scan", () => {
  const primaryListed = { ...setup, destination: setup.sources[0].path };
  assert.deepEqual(incomingSources(primaryListed), [setup.sources[1]]);
  assert.equal(setupProblem(primaryListed), null);
  assert.match(setupProblem({ ...setup, destination: "/synthetic/Laptop/Primary" }), /source contains the primary/);
  assert.match(setupProblem({ ...primaryListed, sources: [setup.sources[0]] }), /Add a source outside/);
  assert.equal(setupProblem({ ...setup, destination: "/synthetic/Laptop-old" }), null);
  assert.match(setupProblem({ ...setup, name: " " }), /name/);
});

test("resuming uses durable plan state and sends changed inventory back to review", () => {
  assert.equal(resumeStep({ plan: null }), "inventory");
  assert.equal(resumeStep({ plan: { state: "approved", generation: 2 }, inventory_progress: { generation: 2 } }), "plan");
  assert.equal(resumeStep({ plan: { state: "paused", generation: 2 }, inventory_progress: { generation: 2 } }), "results");
  assert.equal(resumeStep({ plan: { state: "complete", generation: 1 }, inventory_progress: { generation: 2 } }), "plan");
});

test("comparison pauses and stale plans cannot look like completed copying", () => {
  assert.equal(jobStatus({ ...summary, state: "comparison_paused" }), "Comparison paused — resume");
  assert.equal(jobStatus({ ...summary, state: "comparison_finished_with_errors" }), "Comparison needs attention");
  assert.equal(jobStatus({ ...summary, state: "needs_replan", stage: "plan" }), "Inventory changed — rebuild plan");
});

test("home allows returning to a worker while preventing switching to another job", () => {
  const html = renderToStaticMarkup(createElement(JobsHome, { jobs: [{ ...summary, active: true }, { ...summary, id: "b", name: "Other job" }], loading: false, busy: false, workingJob: "a", error: null, hasDraft: false, onNew() {}, onDraft() {}, onOpen() {}, onRefresh() {} }));
  assert.match(html, /Return to working job/);
  assert.match(html, /aria-label="Open Other job" disabled=""/);
  assert.doesNotMatch(html, /aria-label="Open Family collection" disabled=""/);
});

test("home hides archived jobs by default and offers archive navigation", () => {
  const html = renderToStaticMarkup(createElement(JobsHome, { jobs: [summary, { ...summary, id: "b", name: "Retired collection", archived: true }], loading: false, busy: false, workingJob: null, error: null, hasDraft: false }));
  assert.match(html, /Archive Family collection/);
  assert.match(html, /Archived \(1\)/);
  assert.doesNotMatch(html, /Retired collection/);
});

test("cancelling setup waits for pending saves before discarding it", async () => {
  let persisted = null;
  let release;
  const held = new Promise(resolve => { release = resolve; });
  const writer = createDraftWriter(async value => { await held; persisted = value; });
  const saving = writer.save(setup);
  const cancelling = writer.discard(async () => { persisted = null; });
  release(); await saving; await cancelling;
  assert.equal(persisted, null);
});

test("a failed setup save does not prevent cancellation", async () => {
  let removed = false;
  const writer = createDraftWriter(async () => { throw new Error("cannot save"); });
  await assert.rejects(writer.save(setup));
  await writer.discard(async () => { removed = true; });
  assert.equal(removed, true);
});
