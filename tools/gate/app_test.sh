#!/usr/bin/env bash
# `just app-test`'s body, under the machine lock: the viewer package's tests, then the app's unit tests, then a check
# that no test crashed natively (fix plan 1E). The app's unit-test host installs the native crash handlers and writes
# any dump into `<its temp>/sf-native-crash/<run>/`; the run name goes in as TEST_RUNNER_SF_CRASH_RUN (xcodebuild
# hands TEST_RUNNER_-prefixed variables to the test process without the prefix). A dump there fails this step and
# prints its stack, so a crashing test is a red gate with a stack, not a silent crash dialog.
set -euo pipefail
jobs="${1:-4}"
swift test -j "$jobs" --package-path app/SupportFinsViewer
run="app-test-$(date +%Y%m%d-%H%M%S)-$$"
status=0
TEST_RUNNER_SF_CRASH_RUN="$run" xcodebuild test -jobs "$jobs" -project app/SupportFins.xcodeproj -scheme SupportFins -destination 'platform=macOS' || status=$?
python3 tools/gate/native_dumps.py "$run" || status=1
exit "$status"
