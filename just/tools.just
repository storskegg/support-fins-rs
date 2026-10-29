# Tools: the dashboard and worktree housekeeping.
# New recipes of this area go here; the root justfile only holds shared variables and the imports.

# Local port dashboard at http://127.0.0.1:8787: milestones, tasks and agents (tools/dashboard/state.json) beside live load, build/test processes, worktree activity, PRs and commits; pushed live every 2 s.
dashboard:
    python3 tools/dashboard/server.py

# Keep ~/RustRoverProjects/sfrs-wt small: remove worktrees whose work is on GitHub, drop build output of idle kept ones (dry run; `just tidy --apply` does it).
tidy *args:
    python3 tools/tidy/tidy.py {{args}}

# Rebuild the Help Book (app/SupportFins/Help/SupportFins.help) from the web's docs at the pinned upstream commit; needs pandoc and hiutil.
helpbook:
    deno run --allow-read --allow-write --allow-run=git,pandoc,hiutil --allow-env --node-modules-dir=none tools/helpbook/build.js
