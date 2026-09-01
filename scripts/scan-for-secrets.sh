#!/usr/bin/env bash
# Refuse anything that looks like a credential.
#
# The history was audited once by hand before this repository went public. This
# is what keeps it audited: a file nobody thought about is the one that gets
# committed, and a public repository does not forget.
#
# Reads the paths given, or every tracked file.
set -uo pipefail
cd "$(dirname "$0")/.."

files=("$@")
if [ ${#files[@]} -eq 0 ]; then
    mapfile -t files < <(git ls-files)
fi

# Shapes, not words: a token has a form, and grepping for "password" finds
# every sentence that mentions one.
patterns='ghp_[A-Za-z0-9]{30,}|gho_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}'
patterns="$patterns"'|sk-ant-[A-Za-z0-9-]{40,}|sk-[A-Za-z0-9]{40,}'
patterns="$patterns"'|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9-]{20,}'
patterns="$patterns"'|-----BEGIN [A-Z ]*PRIVATE KEY-----'
patterns="$patterns"'|glpat-[A-Za-z0-9_-]{20,}|AIza[0-9A-Za-z_-]{35}'

found=0
for file in "${files[@]}"; do
    [ -f "$file" ] || continue
    # Nothing is exempt but this file, which has to carry the patterns to match
    # them. The masker's own fixtures used to be exempted, and GitHub's scanner
    # rejected the push anyway — a fake key realistic enough to need an
    # exemption is a fake key that will keep tripping somebody's scanner. They
    # are obviously fake now, so no exemption is needed and none is given.
    case "$file" in
        scripts/scan-for-secrets.sh) continue ;;
    esac

    if hits=$(grep -InE "$patterns" "$file"); then
        echo "::error file=$file::something shaped like a credential"
        echo "$hits" | sed 's/^/    /'
        found=1
    fi
done

if [ "$found" -eq 0 ]; then
    echo "No credential-shaped strings in ${#files[@]} file(s)."
fi

exit "$found"
