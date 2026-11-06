import { invoke } from "@tauri-apps/api/core";

export type Source = { label: string; path: string };
export type Entry = { key: string; root_id: number; relative_path: string; entry_type: "file" | "directory" | "symlink" | "other"; size: number; modified_ns: string | null; link_target: string | null; excluded: boolean; stale: boolean; directory_complete: boolean };
export type Comparison = { key: string; root_id: number; relative_path: string; generation: number; size: number; stage: string; algorithm: string; sample_format: string; sample_hash: string | null; full_hash: string | null; bytes_read: number; failure: string | null; stale: boolean };
export type Snapshot = {
  job_id: string | null; worker: "scan" | "compare" | null; pause_requested: boolean; error: string | null;
  roots: { id: number; path: string }[];
  inventory_progress: { state: string; generation: number; entries: number; stale_entries: number; pending_directories: number; failed_directories: number; failures: number } | null;
  comparison_progress: { state: string; generation: number; files: number; pending_files: number; full_hashes: number; unique_sizes: number; unique_samples: number; failures: number; root_failures: number; bytes_read: number } | null;
  entries: Entry[]; comparisons: Comparison[];
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
};
