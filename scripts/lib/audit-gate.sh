#!/bin/bash

# Shared policy for the dependency audit gates (issue #2091).
#
# An advisory blocks the build when its severity is high or critical, or when
# the severity is unknown (fail closed), and it is not accepted in
# security/audit-allowlist.json with an expiry date that has not passed.
# Medium and low advisories are reported by the scripts but do not block.

audit_gate_blocking() {
  # Reads "<advisory-id><TAB><severity>" lines on stdin and prints the lines
  # that block the build.
  local allowlist="${AUDIT_ALLOWLIST:-$REPO_ROOT/security/audit-allowlist.json}"
  local today allowed="[]" id sev
  today="$(date -u +%F)"
  if [ -f "$allowlist" ]; then
    allowed=$(jq -c --arg today "$today" \
      '[.advisories[]? | select((.expires // "") >= $today) | (.id | tostring)]' "$allowlist") || return 1
  fi
  while IFS=$'\t' read -r id sev; do
    [ -n "${id:-}" ] || continue
    case "${sev:-unknown}" in
      high|critical|unknown) ;;
      *) continue ;;
    esac
    if jq -e --arg id "$id" 'index($id) != null' <<<"$allowed" >/dev/null; then
      continue
    fi
    printf '%s\t%s\n' "$id" "${sev:-unknown}"
  done
}
