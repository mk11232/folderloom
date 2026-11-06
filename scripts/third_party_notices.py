#!/usr/bin/env python3
"""Generate offline, deterministic notices from installed locked production dependencies.

Run npm ci and cargo fetch first. Supplement files are exact upstream texts with
pinned URLs and hashes; adding a dependency without a notice fails closed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "apps/desktop/src-tauri/resources/licenses"
TARGET = "aarch64-apple-darwin"
NOTICE = re.compile(r"^(license|licence|copying|notice|copyright)([._-]|$)", re.I)


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def validate_target():
    tauri_target = os.environ.get("TAURI_ENV_TARGET_TRIPLE", "").strip()
    cargo_target = os.environ.get("CARGO_BUILD_TARGET", "").strip()
    if tauri_target and cargo_target and tauri_target != cargo_target:
        raise SystemExit(f"Conflicting package targets: TAURI_ENV_TARGET_TRIPLE={tauri_target}, "
                         f"CARGO_BUILD_TARGET={cargo_target}")
    target = tauri_target or cargo_target
    if not target:
        host = re.search(r"^host: (.+)$", run("rustc", "-vV"), re.M)
        if not host:
            raise SystemExit("Cannot determine Rust host target for license notices")
        target = host[1].strip()
    if target != TARGET:
        raise SystemExit(f"Unsupported license-notice target: {target}. "
                         f"Only {TARGET} has been reviewed; review dependencies and notices "
                         "for this target before packaging.")


def generate():
    validate_target()
    metadata = json.loads(run("cargo", "metadata", "--locked", "--offline",
                              "--format-version", "1", "--filter-platform", TARGET,
                              "--features", "folderloom-desktop/custom-protocol"))
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    root = next(p["id"] for p in packages.values() if p["name"] == "folderloom-desktop")
    seen = set()

    def visit(identifier):
        if identifier in seen:
            return
        seen.add(identifier)
        for dep in nodes[identifier]["deps"]:
            if any(kind["kind"] != "dev" for kind in dep["dep_kinds"]):
                visit(dep["pkg"])

    visit(root)
    supplemental = json.loads((ROOT / "third-party/supplemental.json").read_text())
    texts = {}
    inventory = []

    def add_text(content):
        key = digest(content.encode())
        texts[key] = content
        return key

    def extras(identifier):
        result = []
        for item in supplemental.get(identifier, []):
            raw = (ROOT / "third-party/upstream" / item["file"]).read_bytes()
            if digest(raw) != item["sha256"]:
                raise ValueError(f"Supplement hash mismatch: {identifier}")
            result.append({"source": item["url"], "text": add_text(raw.decode())})
        return result

    def notice_files(base):
        return [{"source": str(path.relative_to(base)),
                 "text": add_text(path.read_text())}
                for path in sorted(base.rglob("*"))
                if path.is_file() and NOTICE.match(path.name)
                and path.suffix not in {".rs", ".c", ".h"}]

    # Cargo-generated package name/version/checksum fields are quoted ASCII scalars.
    checksums = {}
    for block in (ROOT / "Cargo.lock").read_text().split("[[package]]")[1:]:
        fields = dict(re.findall(r'^(name|version|checksum) = "([a-zA-Z0-9.+_-]+)"$', block, re.M))
        checksums[(fields["name"], fields["version"])] = fields.get("checksum")
    for identifier in sorted(seen, key=lambda i: (packages[i]["name"], packages[i]["version"])):
        package = packages[identifier]
        if package["source"] is None:  # Folderloom's own workspace crates use /LICENSE.
            continue
        name, version = package["name"], package["version"]
        base = Path(package["manifest_path"]).parent
        notices = notice_files(base) + extras(f"{name}@{version}")
        if not package["license"] or not notices:
            raise ValueError(f"Uncovered Rust license: {name}@{version}")
        inventory.append({"ecosystem": "cargo", "name": name, "version": version,
                          "license": package["license"], "authors": package["authors"],
                          "source": f"https://crates.io/api/v1/crates/{name}/{version}/download",
                          "checksum": checksums[(name, version)], "notices": notices})
        if name == "libsqlite3-sys":
            sqlite = (base / "sqlite3/sqlite3.c").read_text()
            start = sqlite.index("/*", sqlite.index("Begin file sqliteInt.h"))
            end = sqlite.index("*/", start) + 2
            inventory.append({"ecosystem": "bundled-c", "name": "SQLite",
                              "version": re.search(r"version (\d+\.\d+\.\d+)", sqlite)[1],
                              "license": "Public domain dedication in upstream source",
                              "source": f"https://crates.io/api/v1/crates/{name}/{version}/download",
                              "notices": [{"source": "sqlite3/sqlite3.c header", "text": add_text(sqlite[start:end])}]})

    frontend = ROOT / "apps/desktop"
    npm = json.loads((frontend / "package-lock.json").read_text())
    for location, locked in sorted(npm["packages"].items()):
        if not location or locked.get("dev"):
            continue
        base = frontend / location
        package = json.loads((base / "package.json").read_text())
        if package["version"] != locked["version"]:
            raise ValueError(f"Run npm ci: installed {location} differs from lock")
        notices = notice_files(base)
        if not notices or not package.get("license"):
            raise ValueError(f"Uncovered npm license: {location}")
        inventory.append({"ecosystem": "npm", "name": package["name"],
                          "version": package["version"], "license": package["license"],
                          "source": locked["resolved"], "integrity": locked["integrity"],
                          "notices": notices})
    inventory.extend([
        {"ecosystem": "icon-origin", "name": "Feather", "version": "4.29.2",
         "license": "MIT", "notices": extras("Feather@4.29.2")},
        {"ecosystem": "license-reference", "name": "MIT terms linked by objc2",
         "license": "MIT", "notices": extras("MIT-terms")},
    ])
    rust = run("rustc", "-vV").strip()
    sysroot = Path(run("rustc", "--print", "sysroot").strip())
    rust_docs = sysroot / "share/doc/rust"
    # Exact toolchain notice covers std and its bundled libraries, including compiler-builtins.
    rust_notice = (rust_docs / "COPYRIGHT-library.html").read_bytes()
    rust_licenses = {f"rust-licenses/{p.name}": p.read_bytes()
                     for p in sorted((rust_docs / "licenses").glob("*.txt"))}
    manifest = {"target": TARGET, "scope": "Desktop production closure plus conservative Rust build dependencies; excludes dev-only npm packages and workspace test dependencies",
                "cargo_lock_sha256": digest((ROOT / "Cargo.lock").read_bytes()),
                "npm_lock_sha256": digest((frontend / "package-lock.json").read_bytes()),
                "rust_toolchain": rust, "rust_notice_sha256": digest(rust_notice),
                "packages": inventory}
    lines = ["Folderloom third-party notices", "================================", "",
             "Generated by scripts/third_party_notices.py. See inventory.json for exact versions,",
             "license expressions, source archive locations, hashes, and notice references.",
             "Rust standard library notices: RUST-STANDARD-LIBRARY.html and rust-licenses/.",
             "Folderloom's own license: FOLDERLOOM-LICENSE.txt.", "",
             "MPL-2.0 source availability: unchanged covered source is available at the exact",
             "crate download URLs listed below and in inventory.json, under MPL-2.0.",
             "Folderloom imposes no additional restrictions on those source rights.",
             "Upstream objc2 LICENSE.md includes an Apple SDK-derived-binding qualification;",
             "it is retained verbatim below. See the repository licensing review for scope.", ""]
    for item in inventory:
        lines += [f"{item['name']} {item.get('version', '')} [{item['license']}]",
                  f"Source: {item.get('source', 'see notice source below')}"]
        for notice in item["notices"]:
            lines.append(f"Notice {notice['text']}: {notice['source']}")
        lines.append("")
    for key, text in sorted(texts.items()):
        lines += [f"===== NOTICE {key} =====", text, ""]
    return {"THIRD-PARTY-NOTICES.txt": "\n".join(lines).encode(),
            "inventory.json": (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode(),
            "RUST-STANDARD-LIBRARY.html": rust_notice,
            "FOLDERLOOM-LICENSE.txt": (ROOT / "LICENSE").read_bytes(), **rust_licenses}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Fail if generated output differs")
    args = parser.parse_args()
    generated = generate()
    for name, data in generated.items():
        path = OUTPUT / name
        if args.check:
            if not path.exists() or path.read_bytes() != data:
                raise SystemExit(f"Notices need regeneration: {name}")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    expected = {OUTPUT / name for name in generated}
    stale = {p for p in OUTPUT.rglob("*") if p.is_file()} - expected
    if stale:
        raise SystemExit("Unexpected stale notice files; review and remove before release")
    print(f"{'Checked' if args.check else 'Generated'} {len(generated)} license resource files")


if __name__ == "__main__":
    main()
