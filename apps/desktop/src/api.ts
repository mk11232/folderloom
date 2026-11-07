import { invoke } from "@tauri-apps/api/core";

export type Source = { label: string; path: string; input_path?: string | null };
export type Entry = { key: string; parent_key: string | null; root_id: number; relative_path: string; entry_type: "file" | "directory" | "symlink" | "other"; size: number; modified_ns: string | null; link_target: string | null; excluded: boolean; stale: boolean; directory_complete: boolean };
export type Comparison = { key: string; root_id: number; relative_path: string; generation: number; size: number; stage: string; algorithm: string; sample_format: string; sample_hash: string | null; full_hash: string | null; bytes_read: number; failure: string | null; stale: boolean };
export type MappingRequest = { source_key: string; destination_relative: string };
export type MergeMapping = MappingRequest & { destination_input: string | null; root_id: number; source_relative: string };
export type PlanItem = { id: number; root_id: number; source_key: string; source_relative: string; destination_relative: string; action: "copy" | "already_present" | "create_directory" | "blocked"; included: boolean; status: "pending" | "verified" | "skipped" | "failed"; detail: string | null; hash: string | null; contributors: { root_id: number; source_relative: string }[] };
export type MergePlan = { id: number; revision: number; approved_revision: number | null; destination: string; destination_input: string | null; generation: number; state: "proposed" | "approved" | "running" | "paused" | "complete" | "finished_with_errors"; mappings: MergeMapping[]; items: PlanItem[] };
export type Snapshot = {
  job_id: string | null; worker: "scan" | "compare" | "plan" | "execute" | null; pause_requested: boolean; recovery_required: boolean; error: string | null;
  roots: { id: number; path: string; input_path: string | null }[];
  inventory_progress: { state: string; generation: number; entries: number; stale_entries: number; pending_directories: number; failed_directories: number; failures: number } | null;
  comparison_progress: { state: string; generation: number; files: number; pending_files: number; full_hashes: number; unique_sizes: number; unique_samples: number; failures: number; root_failures: number; bytes_read: number } | null;
  entries: Entry[]; comparisons: Comparison[]; plan?: MergePlan | null;
  failures: { root_id: number; relative_path: string; message: string; generation: number }[];
  root_failures: { root_id: number; generation: number; message: string; stale: boolean }[];
};
export type Bootstrap = { manifest: { title: string; description: string; roots: Source[] }; manifest_path: string; configuration_error: string | null; snapshot: Snapshot };
export const api = {
  bootstrap: () => invoke<Bootstrap>("desktop_bootstrap"),
  snapshot: () => invoke<Snapshot>("desktop_snapshot"),
  start: (rootPaths: string[]) => invoke<Snapshot>("desktop_start", { rootPaths }),
  run: (mode: "scan" | "compare") => invoke<Snapshot>("desktop_run", { mode }),
  pause: () => invoke<Snapshot>("desktop_pause"),
  plan: (destination: string, mappings: MappingRequest[]) => invoke<Snapshot>("desktop_plan", { destination, mappings }),
  decide: (revision: number, decisions: { item_id: number; included: boolean }[]) => invoke<Snapshot>("desktop_decide", { revision, decisions }),
  approve: (revision: number) => invoke<Snapshot>("desktop_approve", { revision }),
  execute: (revision: number) => invoke<Snapshot>("desktop_execute", { revision }),
  openDestination: () => invoke<void>("desktop_open_destination"),
};
