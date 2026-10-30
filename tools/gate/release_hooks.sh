#!/usr/bin/env bash
# `just app-release-hooks`: a Release build of the app must not carry the UI-test hooks (X-15). They are `#if DEBUG`
# code, so their launch-argument names must not be in the Release binary at all; the same build in Debug must have
# them (otherwise the check proves nothing). Builds in `.dd-release` inside the worktree, under the machine lock.
set -euo pipefail
jobs="${1:-4}"
hooks=(SFChangelogFixture SFCrashDir SFPlantCrash)
dd="$PWD/.dd-release"
build() {
    xcodebuild build -jobs "$jobs" -quiet -project app/SupportFins.xcodeproj -scheme SupportFins \
        -configuration "$1" -derivedDataPath "$dd/$1" -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO >/dev/null
    echo "$dd/$1/Build/Products/$1/Support Fins.app/Contents/MacOS/Support Fins"
}
status=0
rel="$(build Release | tail -1)"
for h in "${hooks[@]}"; do
    if strings -a "$rel" | grep -q -- "$h"; then echo "FAIL: the Release binary contains $h"; status=1; else echo "ok: Release has no $h"; fi
done
dbg="$(build Debug | tail -1)"
# the Debug build keeps its string literals in the debug dylib when previews are on; search the whole bundle
for h in "${hooks[@]}"; do
    if grep -rqa -- "$h" "$(dirname "$dbg")/.." "$(dirname "$dbg")"/*.debug.dylib 2>/dev/null; then echo "ok: Debug has $h"; else echo "FAIL: the Debug build lacks $h (the check is blind)"; status=1; fi
done
exit "$status"
