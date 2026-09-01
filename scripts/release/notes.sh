#!/usr/bin/env bash
# What changed since the last tag, in the words the commits already used.
#
# A release note nobody writes is a release note nobody reads, and one written
# by hand at tag time is written in a hurry. The commit subjects are the notes:
# they were written when the change was fresh and they already say what kind of
# change each one is.
set -euo pipefail
cd "$(dirname "$0")/../.."

tag="${GITHUB_REF_NAME:-$(git describe --tags --abbrev=0 2>/dev/null || echo HEAD)}"
previous=$(git describe --tags --abbrev=0 "${tag}^" 2>/dev/null || true)

if [ -n "$previous" ]; then
    range="${previous}..${tag}"
    echo "## What changed since ${previous}"
else
    range="$tag"
    echo "## The first release"
fi
echo

# Grouped by the conventional type, because "what is new" and "what stopped
# being broken" are read by different people for different reasons.
emit() {
    local title="$1" pattern="$2" lines
    lines=$(git log --no-merges --format='%s' "$range" | grep -E "$pattern" || true)
    [ -z "$lines" ] && return 0

    echo "### $title"
    echo
    # The type prefix has done its job by now; the subject is the sentence.
    echo "$lines" | sed -E 's/^[a-z]+(\([^)]*\))?: //' | sed 's/^/- /'
    echo
}

emit "New" '^feat(\(|:)'
emit "Fixed" '^fix(\(|:)'
emit "Faster or leaner" '^perf(\(|:)'
emit "Inside" '^(refactor|chore|test|style)(\(|:)'
emit "Written down" '^docs(\(|:)'

echo "### Updating"
echo
echo "The app checks for updates on its own and Settings has a button to check now."
echo "Every bundle is signed; one that is not, or one signed by anything else, is refused."
