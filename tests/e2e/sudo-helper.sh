#!/usr/bin/env bash
# /usr/local/sbin/isoboot-e2e — installed by install-sudo-helper.sh.
#
# Thin root-side wrapper that runs the isoboot end-to-end test runner.
# Intended to be paired with a NOPASSWD sudoers entry restricted to this
# exact path so the test can be invoked without an interactive prompt.
#
# This helper deliberately does NOT accept arbitrary commands. It only
# exec's the test runner in the isoboot repo.

set -euo pipefail

REPO="/home/ngupta/src/isoboot"
TEST="$REPO/tests/e2e/run.sh"

if [[ ! -f "$TEST" ]]; then
    echo "isoboot-e2e: missing test runner: $TEST" >&2
    exit 1
fi

# Forward all args verbatim to the test runner.
exec /usr/bin/env bash "$TEST" "$@"
