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
    # The masker's own tests carry deliberately fake keys, and so does the
    # README where it shows what the masker catches. They are the one place a
    # key-shaped string belongs.
    case "$file" in
        crates/core/src/memory.rs|README.md|scripts/scan-for-secrets.sh) continue ;;
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
