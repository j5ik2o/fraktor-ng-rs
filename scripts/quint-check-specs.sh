#!/usr/bin/env bash
# Checks every crates/*/specs/*.qnt with the quint pinned in package-lock.json:
# typecheck, every `run` as a test, and a short simulation of `invariant`.
# Run from the repository root. CI runs it in the `quint` job of ci.yml.
# Design: docs/design/quint-in-ci.md.
set -uo pipefail
shopt -s nullglob

quint=./node_modules/.bin/quint
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

fail() {
  echo "::error file=$1::$2"
  return 1
}

# The module quint treats as main when no --main is given: the file's only
# module, else the one named after the file. `quint parse` also lists imported
# modules, so a file whose single module is named otherwise and that imports
# another file is not resolved here and fails instead of being guessed.
main_module() {
  jq -r --arg base "$2" '
    [.modules[].name] as $names
    | if ($names | index($base)) != null then $base
      elif ($names | length) == 1 then $names[0]
      else empty end' "$1"
}

check_spec() {
  local spec=$1 base ir main out runs=() bad=() name passing max
  base=$(basename "$spec" .qnt)
  ir=$work/$base.ir.json
  out=$work/$base.out

  "$quint" typecheck "$spec" || return 1
  "$quint" parse --out="$ir" "$spec" >/dev/null || return 1
  main=$(main_module "$ir" "$base")
  [ -n "$main" ] || fail "$spec" "cannot tell the main module: none is named $base and the file has more than one" || return 1

  # quint test, without --match, runs only definitions whose name ends in Test.
  # Every run must be one of them, so a run is never left out silently.
  while IFS= read -r name; do
    runs+=("$name")
    case $name in *Test) ;; *) bad+=("$name") ;; esac
  done < <(jq -r --arg m "$main" '.modules[] | select(.name == $m) | .declarations[] | select(.qualifier == "run") | .name' "$ir")
  [ ${#runs[@]} -gt 0 ] || fail "$spec" "module $main defines no run; every spec needs at least one" || return 1
  [ ${#bad[@]} -eq 0 ] || fail "$spec" "quint test skips these runs because their names do not end in Test: ${bad[*]}" || return 1

  "$quint" test "$spec" | tee "$out" || return 1
  passing=$(sed -nE 's/^ *([0-9]+) passing.*/\1/p' "$out")
  [ "$passing" = "${#runs[@]}" ] || fail "$spec" "quint test passed ${passing:-0} test(s) but module $main defines ${#runs[@]} run(s)" || return 1

  # `invariant` is required: a spec without it fails with QNT404. 20 steps and
  # 10000 traces (quint's default) cost about one second for three specs. The
  # seed is random on each run; a violation prints the seed that reproduces it.
  "$quint" run --invariant=invariant --max-steps=20 --max-samples=10000 --verbosity=2 "$spec" | tee "$out" || return 1
  # max=1 means no trace took a step, so `invariant` was checked on initial states only.
  max=$(sed -nE 's/^Trace length statistics: max=([0-9]+),.*/\1/p' "$out")
  [ -n "$max" ] || fail "$spec" "quint run printed no trace length statistics" || return 1
  [ "$max" -gt 1 ] || fail "$spec" "no trace took a step (max trace length $max): step is never enabled" || return 1
}

specs=(crates/*/specs/*.qnt)
if [ ${#specs[@]} -eq 0 ]; then
  echo '::error::No Quint spec matched crates/*/specs/*.qnt'
  exit 1
fi

failed=()
for spec in "${specs[@]}"; do
  echo "::group::$spec"
  check_spec "$spec" || failed+=("$spec")
  echo '::endgroup::'
done

if [ ${#failed[@]} -gt 0 ]; then
  for spec in "${failed[@]}"; do
    echo "::error file=$spec::quint check failed (see the group for $spec above)"
  done
  exit 1
fi
