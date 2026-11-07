import type { JobSetup, JobSummary, Snapshot } from "./api.ts";

export function pathsOverlap(first: string, second: string) {
  if (!first || !second) return false;
  const a = first.replace(/\/+$/, "") || "/";
  const b = second.replace(/\/+$/, "") || "/";
  return a === b || a === "/" || b === "/" || a.startsWith(`${b}/`) || b.startsWith(`${a}/`);
}
export const emptySetup = (): JobSetup => ({ name: "", sources: [], destination: "" });
export function incomingSources(setup: JobSetup) { return setup.sources.filter(source => !pathsOverlap(source.path, setup.destination)); }
export function setupProblem(setup: JobSetup) {
  if (!setup.name.trim()) return "Give this job a name.";
  if (!setup.sources.length) return "Add at least one source folder.";
  if (!setup.destination) return "Choose the primary destination folder.";
  if (setup.sources.some(source => pathsOverlap(source.path, setup.destination) && source.path.replace(/\/+$/, "") !== setup.destination.replace(/\/+$/, ""))) return "A source contains the primary destination, or sits inside it. Remove that source or choose a separate destination.";
  if (!incomingSources(setup).length) return "Add a source outside the primary destination. Primary and overlapping folders stay out of the incoming scan.";
  return null;
}
export function resumeStep(snapshot: Snapshot): "inventory" | "plan" | "results" {
  if (!snapshot.plan) return "inventory";
  if (snapshot.inventory_progress && snapshot.plan.generation !== snapshot.inventory_progress.generation) return "plan";
  return ["running", "paused", "complete", "finished_with_errors"].includes(snapshot.plan.state) ? "results" : "plan";
}
export function jobStatus(job: JobSummary) {
  if (job.state === "needs_replan") return "Inventory changed — rebuild plan";
  if (job.state.startsWith("comparison_")) return ({ comparison_running: job.active ? "Comparing contents" : "Comparison interrupted — resume", comparison_paused: "Comparison paused — resume", comparison_finished_with_errors: "Comparison needs attention", comparison_complete: "Comparison ready" }[job.state] ?? job.state.replaceAll("_", " "));
  const area = job.stage === "results" ? "Copy" : job.stage === "plan" ? "Plan" : "Inventory";
  if (job.active) return `${area} in progress`;
  if (job.error || job.state === "finished_with_errors") return `${area} needs attention`;
  if (job.state === "complete") return job.stage === "results" ? "Copy results ready" : job.stage === "plan" ? "Plan ready" : "Inventory ready";
  if (job.state === "paused" || job.state === "running") return `${area} paused — resume`;
  if (job.state === "approved") return "Plan approved";
  if (job.state === "proposed") return "Plan ready for review";
  return job.state.replaceAll("_", " ");
}

// Serialize saves so a slower earlier write cannot overwrite a later removal.
// A failed save does not poison subsequent retries; flush reports the latest save.
export function createDraftWriter(write: (setup: JobSetup) => Promise<void>) {
  let pending = Promise.resolve();
  return {
    save(setup: JobSetup) {
      const copy = { ...setup, sources: setup.sources.map(source => ({ ...source })) };
      pending = pending.catch(() => undefined).then(() => write(copy));
      return pending;
    },
    flush() { return pending; },
  };
}
