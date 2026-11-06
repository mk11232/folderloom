import assert from "node:assert/strict";
import test from "node:test";
import { fullHashLocations } from "../src/matching-locations.ts";

const file = (key, root_id, relative_path, extra = {}) => ({ key, root_id, relative_path, entry_type: "file", stale: false, excluded: false, ...extra });
const evidence = (entry, extra = {}) => ({ key: entry.key, root_id: entry.root_id, relative_path: entry.relative_path, generation: 1, algorithm: "blake3", full_hash: "same-full-content", sample_hash: "same-sample", stage: "full_hash", stale: false, ...extra });

const copies = [
  file("one", 1, "Documents/report.pdf"),
  file("two", 2, "Backup/report.pdf"),
  file("three", 3, "report-renamed.pdf"),
  file("four", 4, "Archive/report.pdf"),
];

test("selecting any one of four placements shows every matching location, including itself", () => {
  const locations = fullHashLocations(copies, copies.map(entry => evidence(entry)));
  for (const selected of copies) {
    assert.deepEqual(locations.get(selected.key).map(record => [record.key, record.root_id, record.relative_path]), copies.map(entry => [entry.key, entry.root_id, entry.relative_path]));
  }
  // A source or path search selects one visible row without narrowing its location list.
  const visible = copies.filter(entry => entry.root_id === 2 && entry.relative_path.includes("report"));
  assert.equal(visible.length, 1);
  assert.equal(locations.get(visible[0].key).length, 4);
});

test("stale inventory, stale hashes, samples and unfinished reads never appear as matches", () => {
  const entries = [...copies, file("sample", 5, "sample.pdf"), file("reading", 6, "reading.pdf")];
  const records = entries.map(entry => evidence(entry));
  const locations = fullHashLocations(entries.map(entry => entry.key === "three" ? { ...entry, stale: true } : entry), records.map(record => ({ ...record,
    ...(record.key === "four" ? { stale: true } : {}),
    ...(record.key === "sample" ? { stage: "sampled" } : {}),
    ...(record.key === "reading" ? { stage: "hashing" } : {}),
  })));
  assert.deepEqual(locations.get("one").map(record => record.key), ["one", "two"]);
  for (const key of ["three", "four", "sample", "reading"]) assert.equal(locations.get(key), undefined);
});

test("newer comparison evidence replaces old matches and algorithms remain distinct", () => {
  const records = copies.map(entry => evidence(entry));
  records.push(evidence(copies[1], { generation: 2, stage: "pending_sample", full_hash: null }));
  records[2] = evidence(copies[2], { algorithm: "different-algorithm" });
  records[3] = evidence(copies[3], { full_hash: "different-content" });
  const locations = fullHashLocations(copies, records);
  for (const entry of copies) assert.equal(locations.get(entry.key), undefined);
});
