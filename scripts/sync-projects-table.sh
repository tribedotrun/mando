#!/usr/bin/env bash
# One-time script: populate projects table from config.json (all fields).
# Run after merging the projects-table migration.
#
# Usage: ./scripts/sync-projects-table.sh [db-path]
#   db-path defaults to ~/.mando/mando.db

set -euo pipefail

DB="${1:-$HOME/.mando/mando.db}"
CONFIG="$HOME/.mando/config.json"

if [ ! -f "$DB" ]; then
  echo "error: database not found at $DB" >&2
  exit 1
fi
if [ ! -f "$CONFIG" ]; then
  echo "error: config not found at $CONFIG" >&2
  exit 1
fi

echo "db:     $DB"
echo "config: $CONFIG"
echo

python3 - "$DB" "$CONFIG" << 'PYEOF'
import json, sqlite3, sys

db_path, config_path = sys.argv[1], sys.argv[2]

with open(config_path) as f:
    config = json.load(f)

projects = config.get("captain", {}).get("projects", {})
if not projects:
    print("no projects found in config")
    sys.exit(0)

conn = sqlite3.connect(db_path)
cur = conn.cursor()
now = (__import__("datetime").datetime.now(__import__("datetime").timezone.utc)).strftime("%Y-%m-%dT%H:%M:%SZ")

for key, proj in projects.items():
    name = proj.get("name", "")
    if not name:
        continue

    path = proj.get("path", "")
    github_repo = proj.get("githubRepo") or None
    aliases = json.dumps(proj.get("aliases", []))
    hooks = json.dumps(proj.get("hooks", {}))
    worker_preamble = proj.get("workerPreamble", "")
    check_command = proj.get("checkCommand", "")
    logo = proj.get("logo") or None
    scout_summary = proj.get("scoutSummary", "")
    classify_rules = json.dumps(proj.get("classifyRules", []))

    cur.execute(
        """INSERT INTO projects
           (name, path, github_repo, aliases, hooks, worker_preamble,
            check_command, logo, scout_summary, classify_rules, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT(name) DO UPDATE SET
             path=excluded.path, github_repo=excluded.github_repo,
             aliases=excluded.aliases, hooks=excluded.hooks,
             worker_preamble=excluded.worker_preamble,
             check_command=excluded.check_command, logo=excluded.logo,
             scout_summary=excluded.scout_summary,
             classify_rules=excluded.classify_rules,
             updated_at=excluded.updated_at""",
        (name, path, github_repo, aliases, hooks, worker_preamble,
         check_command, logo, scout_summary, classify_rules, now, now),
    )
    print(f"  upserted: {name}")

conn.commit()

print()
print("projects table:")
for row in cur.execute(
    "SELECT id, name, path, github_repo, aliases FROM projects"
):
    print(f"  {row[0]:>3} | {row[1]:<15} | {row[2]:<50} | {row[3]} | aliases={row[4]}")

conn.close()
PYEOF
