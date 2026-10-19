#!/usr/bin/env bash
#
# check_readme_contract_table.sh
#
# Verifies that the "Contract Table" in README.md matches the exported
# contract functions declared in the #[contractimpl] block of src/lib.rs.
#
# It parses each exported function's name and parameter list from src/lib.rs
# and compares them against the rows of the README contract table. The script
# fails (non-zero exit) when:
#   - a README table row references a function that is not exported by the
#     contract, or
#   - a README table row's parameter list has drifted from the real signature,
#     or
#   - an exported contract function is missing from the README table.
#
# Usage: scripts/check_readme_contract_table.sh [path/to/lib.rs] [path/to/README.md]
#
set -euo pipefail

LIB_RS="${1:-src/lib.rs}"
README="${2:-README.md}"

if [[ ! -f "$LIB_RS" ]]; then
  echo "error: contract source not found: $LIB_RS" >&2
  exit 2
fi

if [[ ! -f "$README" ]]; then
  echo "error: README not found: $README" >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# Extract exported contract functions from the #[contractimpl] block.
#
# We locate the #[contractimpl] attribute, then walk the following block,
# tracking brace depth, and collect `pub fn <name>(<params>)` signatures.
# ---------------------------------------------------------------------------
extract_contract_fns() {
  awk '
    BEGIN {
      in_impl = 0
      depth = 0
      started = 0
    }

    # Detect the #[contractimpl] attribute.
    /#\[contractimpl\]/ {
      in_impl = 1
      next
    }

    in_impl && !started {
      # Wait for the opening brace of the impl block.
      if (index($0, "{") > 0) {
        started = 1
        # Count braces on this line.
        line = $0
        for (i = 1; i <= length(line); i++) {
          c = substr(line, i, 1)
          if (c == "{") depth++
          else if (c == "}") depth--
        }
      }
      next
    }

    in_impl && started {
      # Try to capture a function signature. Signatures may span lines, so we
      # accumulate until we see the opening paren and its matching close.
      line = $0

      # Accumulate multi-line signatures.
      if (accumulating) {
        sig = sig " " line
      } else if (line ~ /(^|[^A-Za-z0-9_])fn[ \t]+[A-Za-z0-9_]+/) {
        sig = line
        accumulating = 1
      }

      if (accumulating) {
        # Once we have a balanced paren group, emit the signature.
        open = 0
        close = 0
        for (i = 1; i <= length(sig); i++) {
          c = substr(sig, i, 1)
          if (c == "(") open++
          else if (c == ")") close++
        }
        if (open > 0 && open == close) {
          # Extract name and params.
          if (match(sig, /fn[ \t]+[A-Za-z0-9_]+[ \t]*\(/)) {
            head = substr(sig, RSTART, RLENGTH)
            # name
            name = head
            sub(/^fn[ \t]+/, "", name)
            sub(/[ \t]*\($/, "", name)
            # params: everything between first ( and its matching )
            pstart = index(sig, "(")
            # find matching close paren
            pdepth = 0
            pend = 0
            for (i = pstart; i <= length(sig); i++) {
              c = substr(sig, i, 1)
              if (c == "(") pdepth++
              else if (c == ")") {
                pdepth--
                if (pdepth == 0) { pend = i; break }
              }
            }
            params = substr(sig, pstart + 1, pend - pstart - 1)
            print name "\t" params
          }
          accumulating = 0
          sig = ""
        }
      }

      # Track brace depth to know when the impl block ends.
      for (i = 1; i <= length(line); i++) {
        c = substr(line, i, 1)
        if (c == "{") depth++
        else if (c == "}") depth--
      }
      if (depth <= 0) {
        in_impl = 0
        started = 0
      }
    }
  ' "$LIB_RS"
}

# ---------------------------------------------------------------------------
# Normalize a parameter list: split on top-level commas, strip types/patterns,
# and keep only the parameter names. Handles `mut`, `&self`, `&mut self`,
# references, and generic types with commas.
# ---------------------------------------------------------------------------
normalize_params() {
  local raw="$1"
  # Remove surrounding whitespace.
  raw="$(printf '%s' "$raw" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
  if [[ -z "$raw" ]]; then
    echo ""
    return
  fi

  # Split on commas that are not inside angle brackets or parentheses.
  local params=()
  local current=""
  local depth=0
  local i c
  for (( i=0; i<${#raw}; i++ )); do
    c="${raw:$i:1}"
    case "$c" in
      '<'|'('|'[') depth=$((depth+1)); current+="$c" ;;
      '>'|')'|']') depth=$((depth-1)); current+="$c" ;;
      ',')
        if (( depth == 0 )); then
          params+=("$current")
          current=""
        else
          current+="$c"
        fi
        ;;
      *) current+="$c" ;;
    esac
  done
  if [[ -n "$current" ]]; then
    params+=("$current")
  fi

  local names=()
  local p name
  for p in "${params[@]}"; do
    p="$(printf '%s' "$p" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
    [[ -z "$p" ]] && continue
    # Drop `&self` / `&mut self` / `self`.
    if [[ "$p" =~ ^(&[[:space:]]*)?(mut[[:space:]]+)?self$ ]]; then
      continue
    fi
    # Strip leading `mut `.
    p="$(printf '%s' "$p" | sed -E 's/^mut[[:space:]]+//')"
    # Parameter name is the token before the first `:`.
    if [[ "$p" == *:* ]]; then
      name="${p%%:*}"
    else
      name="$p"
    fi
    name="$(printf '%s' "$name" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
    # Strip leading `&`/`mut` from the name token if present.
    name="$(printf '%s' "$name" | sed -E 's/^&[[:space:]]*//; s/^mut[[:space:]]+//')"
    names+=("$name")
  done

  local IFS=','
  echo "${names[*]}"
}

# Build a lookup of contract functions: name -> normalized params.
declare -A CONTRACT_PARAMS=()
declare -A CONTRACT_SEEN=()

while IFS=$'\t' read -r fname fparams; do
  [[ -z "$fname" ]] && continue
  norm="$(normalize_params "$fparams")"
  CONTRACT_PARAMS["$fname"]="$norm"
  CONTRACT_SEEN["$fname"]=1
done < <(extract_contract_fns)

if [[ ${#CONTRACT_PARAMS[@]} -eq 0 ]]; then
  echo "error: no exported contract functions found in $LIB_RS (#[contractimpl] block)" >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# Parse the README contract table.
#
# We look for a markdown table whose header mentions "Function" (case
# insensitive) and a "Parameters" column. Rows are parsed until the table ends.
# ---------------------------------------------------------------------------
declare -A README_PARAMS=()
declare -A README_SEEN=()
declare -a README_ORDER=()

in_table=0
header_seen=0
func_col=-1
param_col=-1
line_no=0

while IFS= read -r line; do
  line_no=$((line_no + 1))

  # Detect a table header row containing "function".
  if [[ $in_table -eq 0 ]]; then
    if [[ "$line" =~ ^[[:space:]]*\| ]] && \
       printf '%s' "$line" | grep -qiE '\|[[:space:]]*function[[:space:]]*\|'; then
      in_table=1
      header_seen=0
      # Parse header columns.
      IFS='|' read -r -a cols <<< "$line"
      func_col=-1
      param_col=-1
      idx=0
      for col in "${cols[@]}"; do
        cnorm="$(printf '%s' "$col" | tr '[:upper:]' '[:lower:]' | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
        if [[ "$cnorm" == "function" ]]; then
          func_col=$idx
        elif [[ "$cnorm" == "parameters" || "$cnorm" == "params" ]]; then
          param_col=$idx
        fi
        idx=$((idx + 1))
      done
      continue
    fi
    continue
  fi

  # End of table: a non-table line.
  if [[ ! "$line" =~ ^[[:space:]]*\| ]]; then
    in_table=0
    continue
  fi

  # Skip the separator row (e.g. |---|---|).
  if printf '%s' "$line" | grep -qE '^[[:space:]]*\|[[:space:]]*:?-+:?[[:space:]]*\|'; then
    header_seen=1
    continue
  fi

  # Skip until we have seen the separator (i.e. header + separator).
  if [[ $header_seen -eq 0 ]]; then
    continue
  fi

  IFS='|' read -r -a cols <<< "$line"
  # Extract function cell.
  if (( func_col < 0 || func_col >= ${#cols[@]} )); then
    continue
  fi
  fcell="${cols[$func_col]}"
  # Strip markdown formatting: backticks, bold, links, whitespace.
  fcell="$(printf '%s' "$fcell" | sed -E 's/`//g; s/\*\*//g; s/^[[:space:]]*//; s/[[:space:]]*$//')"
  # Extract the function name (first identifier-like token).
  if [[ "$fcell" =~ ([A-Za-z_][A-Za-z0-9_]*) ]]; then
    rname="${BASH_REMATCH[1]}"
  else
    continue
  fi

  # Extract parameters cell if present.
  rparams=""
  if (( param_col >= 0 && param_col < ${#cols[@]} )); then
    pcell="${cols[$param_col]}"
    pcell="$(printf '%s' "$pcell" | sed -E 's/`//g; s/\*\*//g; s/^[[:space:]]*//; s/[[:space:]]*$//')"
    rparams="$(normalize_params "$pcell")"
  fi

  README_PARAMS["$rname"]="$rparams"
  README_SEEN["$rname"]=1
  README_ORDER+=("$rname")
done < "$README"

if [[ ${#README_PARAMS[@]} -eq 0 ]]; then
  echo "error: no contract table rows found in $README" >&2
  echo "       expected a markdown table with 'Function' and 'Parameters' columns" >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# Compare.
# ---------------------------------------------------------------------------
errors=0

# 1. Every README row must reference an exported contract function.
for rname in "${README_ORDER[@]}"; do
  if [[ -z "${CONTRACT_SEEN[$rname]:-}" ]]; then
    echo "error: README contract table lists '$rname', which is not an exported contract function in $LIB_RS" >&2
    errors=$((errors + 1))
    continue
  fi

  # 2. Parameter lists must match.
  expected="${CONTRACT_PARAMS[$rname]}"
  actual="${README_PARAMS[$rname]}"
  if [[ "$expected" != "$actual" ]]; then
    echo "error: signature drift for '$rname'" >&2
    echo "       contract: ($expected)" >&2
    echo "       README:   ($actual)" >&2
    errors=$((errors + 1))
  fi
done

# 3. Every exported contract function must appear in the README table.
for fname in "${!CONTRACT_SEEN[@]}"; do
  if [[ -z "${README_SEEN[$fname]:-}" ]]; then
    echo "error: exported contract function '$fname' is missing from the README contract table" >&2
    errors=$((errors + 1))
  fi
done

if (( errors > 0 )); then
  echo "" >&2
  echo "README contract table is out of sync with $LIB_RS ($errors issue(s))." >&2
  echo "Update the table so each row matches the exported signature." >&2
  exit 1
fi

echo "README contract table matches exported contract functions in $LIB_RS."
exit 0