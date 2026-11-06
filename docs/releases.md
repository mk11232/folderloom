# Release practices

Folderloom uses CalVer `YYYY.M.release`, starting at `2026.11.1`. The year and month describe publication, not the date coding began. Increment the final number for each release in that month; reset it to 1 in a new month. Do not pad numbers with zeroes. Tags use `v2026.11.1` format. A version change alone does not publish a release.

Keep the workspace version in `Cargo.toml`, the three workspace package records in `Cargo.lock`, `apps/desktop/package.json`, its lockfile root record, and `apps/desktop/src-tauri/tauri.conf.json` synchronized. Regenerate dependency notices after lockfile changes. Inspect the packaged app's version and bundled notices before distribution.

CalVer does not promise API or database compatibility. Document format migrations, supported platforms, and behavior changes explicitly. The current version is an unsigned development demo; no public binary release or tag is created merely by merging its code.

The main branch requires pull requests, resolved review conversations, an up-to-date branch, and both GitHub Actions checks: `Inventory (macos-latest)` and `Inventory (ubuntu-latest)`. Rules apply to administrators, and force pushes and branch deletion are disabled. Required human approval count is zero while the repository is maintained by one owner; independent implementation reviews remain part of the working process. If private-repository plan capabilities change, verify protections remain enforced.
