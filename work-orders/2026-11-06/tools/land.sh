#!/usr/bin/env bash
# usage: land.sh NN "commit subject" ["body"]   Commits everything in the worktree and pushes its branch.
set -eu
BASE="C:/Users/roore/AppData/Local/Temp/claude/5ba796caee228558/2a7d3671-feaa-438a-b6cc-b31852ae8e8e/scratchpad/wo"
cd "$BASE/t/$1"
git add -A
git status --short | grep -E "node_modules|\.png$|packages/koharu/AGENTS.md" && { echo "unexpected files staged"; exit 2; } || true
git -c core.safecrlf=false -c user.name=roorem10 -c user.email=roorem10@users.noreply.github.com commit -q -m "$2" -m "${3:-Built by Codex from a work order, reviewed by reading. Rust not compiled yet; bindings hand-updated and need regenerating.}" 2>/dev/null
git push -q -u origin HEAD 2>&1 | grep -v "^remote:" || true
git log -1 --format='%h %s'; git status -sb | head -1; git diff --stat HEAD~1 | tail -1
