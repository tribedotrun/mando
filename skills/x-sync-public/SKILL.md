---
name: x-sync-public
description: Sync mando-private to the public tribedotrun/mando repo. Strips private paths, patches Cargo.toml, generates commit message from code delta.
---

## Workflow

1. **Prepare** — run `mando-dev sync-public prepare`. This copies main (minus `.public-ignore` paths) to a staging clone, patches Cargo.toml, regenerates Cargo.lock, and stages everything. If exit code is non-zero, the public repo is already up to date — stop.

2. **Read the delta** — run `mando-dev sync-public diff --stat` to get the file-level summary. Then run `mando-dev sync-public diff` to read the actual code diff. If the diff is large, read the first ~500 lines.

3. **Generate commit message** — write a single-line commit message (max 140 chars) based on the code delta. Rules:
   - No conventional-commit prefix (`feat:`, `fix:`, etc.)
   - No PR numbers or issue references
   - Must read like normal human development — **never mention sync, mirror, or private repo**
   - Focus on what the code changes DO, not file names
   - Show the proposed message to the human for approval

4. **Commit and push** — after human approves, run `mando-dev sync-public commit "<message>"`.

## Notes

- Must be on `main` with a clean working tree
- The staging clone lives at `~/.cache/mando-dev/sync-public`
- Push goes via SSH to bypass the global `pushInsteadOf` guard
