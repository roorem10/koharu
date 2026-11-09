#!/usr/bin/env bash
# usage: run.sh NN branch-name  (WO text in NN.md). Creates the worktree if needed and runs Codex.
set -u
BASE="C:/Users/roore/AppData/Local/Temp/claude/5ba796caee228558/2a7d3671-feaa-438a-b6cc-b31852ae8e8e/scratchpad/wo"
NN=$1; BR=$2; WT="$BASE/t/$NN"; REPO="C:/Users/roore/Documents/koharu"
mkdir -p "$BASE/logs"
if [ ! -d "$WT" ]; then
  git -C "$REPO" worktree add -q -b "$BR" "$WT" booharu || exit 3
  node "$BASE/link-modules.js" "$REPO" "$WT" || exit 4
fi
cat "$BASE/preamble.md" "$BASE/$NN.md" > "$BASE/logs/$NN.prompt.md"
cd "$WT" || exit 5
date -u +"START %H:%M:%S" > "$BASE/logs/$NN.log"
/c/Users/roore/AppData/Roaming/npm/codex.cmd exec --sandbox workspace-write - < "$BASE/logs/$NN.prompt.md" >> "$BASE/logs/$NN.log" 2>&1
rc=$?
date -u +"END %H:%M:%S rc=$rc" >> "$BASE/logs/$NN.log"
echo "WO $NN finished rc=$rc, changed files: $(git -C "$WT" status --short | wc -l)"
tail -40 "$BASE/logs/$NN.log"
exit $rc
