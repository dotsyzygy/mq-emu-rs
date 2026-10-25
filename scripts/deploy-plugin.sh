#!/usr/bin/env bash
# Bash wrapper for deploy-plugin.ps1: builds a plugin and copies it (and any
# Lua scripts in its lua/ folder) into MacroQuest. See that script for details.
#
# Usage: scripts/deploy-plugin.sh [--plugin NAME] [--debug] [--no-build] [-h|--help]
# PowerShell-style arguments (-Plugin NAME, -DebugBuild, -NoBuild) pass through unchanged.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

args=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --plugin)
            [[ $# -ge 2 ]] || { echo "--plugin needs a name" >&2; exit 2; }
            args+=(-Plugin "$2")
            shift 2
            ;;
        --plugin=*) args+=(-Plugin "${1#*=}"); shift ;;
        --debug) args+=(-DebugBuild); shift ;;
        --no-build) args+=(-NoBuild); shift ;;
        -h|--help)
            sed -n '2,6p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) args+=("$1"); shift ;;
    esac
done

if command -v pwsh >/dev/null 2>&1; then
    shell=pwsh
elif command -v powershell.exe >/dev/null 2>&1; then
    shell=powershell.exe
else
    echo "PowerShell not found (looked for pwsh and powershell.exe)" >&2
    exit 1
fi

# cygpath gives PowerShell a Windows path under Git Bash/MSYS; elsewhere the path is used as is.
ps1="$script_dir/deploy-plugin.ps1"
if command -v cygpath >/dev/null 2>&1; then
    ps1="$(cygpath -w "$ps1")"
fi

exec "$shell" -NoProfile -ExecutionPolicy Bypass -File "$ps1" "${args[@]}"
