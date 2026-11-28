#!/usr/bin/env bash
# Derives the output examples of docs/design/issue-183-metrics-kit.md §2 from
# the original PekkoConsoleReporter: compiles the Pekko v2.0.0 sources of the
# reporter and the metrics it prints with Scala 2.13.18 (the first of Pekko's
# publishedScalaVersions, so the one its tests run on), runs
# ConsoleReporterExamples.scala, and compares the output with
# console-reporter-examples.expected. Then it runs one case with the Windows
# line separator and compares that with the "crlf" section of the same file.
#
# Needs: bash, git, curl, shasum or sha1sum, and Java 17 or later. The Pekko
# clone is $PEKKO_REPO (default ~/Sources/github.com/j5ik2o/pekko); only
# `git show v2.0.0:<path>` is read from it, so its work tree may be on any
# version. The jars come from Maven Central and are checked against the
# .sha1 files next to them.
#
#   docs/design/issue-299/derive-console-reporter-examples.sh          # compare
#   docs/design/issue-299/derive-console-reporter-examples.sh --print  # print only
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
pekko=${PEKKO_REPO:-$HOME/Sources/github.com/j5ik2o/pekko}
tag=v2.0.0
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

central=https://repo1.maven.org/maven2
jars=(
  org/scala-lang/scala-library/2.13.18/scala-library-2.13.18.jar
  org/scala-lang/scala-reflect/2.13.18/scala-reflect-2.13.18.jar
  org/scala-lang/scala-compiler/2.13.18/scala-compiler-2.13.18.jar
  io/github/java-diff-utils/java-diff-utils/4.16/java-diff-utils-4.16.jar
  io/dropwizard/metrics/metrics-core/4.2.40/metrics-core-4.2.40.jar
  io/dropwizard/metrics/metrics-jvm/4.2.40/metrics-jvm-4.2.40.jar
  org/hdrhistogram/HdrHistogram/2.2.2/HdrHistogram-2.2.2.jar
  org/slf4j/slf4j-api/1.7.36/slf4j-api-1.7.36.jar
)

sha1() {
  if command -v sha1sum >/dev/null; then sha1sum "$1" | cut -d' ' -f1; else shasum -a 1 "$1" | cut -d' ' -f1; fi
}

mkdir -p "$work/lib" "$work/src" "$work/classes"
cp=""
for jar in "${jars[@]}"; do
  file="$work/lib/$(basename "$jar")"
  curl -sSfL -o "$file" "$central/$jar"
  expected=$(curl -sSfL "$central/$jar.sha1" | cut -c1-40)
  [ "$(sha1 "$file")" = "$expected" ] || { echo "sha1 mismatch: $jar" >&2; exit 1; }
  cp="$cp${cp:+:}$file"
done

metrics=testkit/src/test/scala/org/apache/pekko/testkit/metrics
for path in \
  $metrics/AveragingGauge.scala \
  $metrics/HdrHistogram.scala \
  $metrics/KnownOpsInTimespanTimer.scala \
  $metrics/FileDescriptorMetricSet.scala \
  $metrics/reporter/PekkoConsoleReporter.scala \
  actor/src/main/scala/org/apache/pekko/util/PrettyDuration.scala; do
  git -C "$pekko" show "$tag:$path" >"$work/src/$(basename "$path")"
done
# PekkoMetricRegistry is lines 203-217 of MetricsKit.scala; the rest of that
# file needs scalatest and the config library, which the reporter does not.
{
  echo "package org.apache.pekko.testkit.metrics"
  echo "import scala.collection.mutable"
  echo "import com.codahale.metrics._"
  git -C "$pekko" show "$tag:$metrics/MetricsKit.scala" | sed -n '203,217p'
} >"$work/src/PekkoMetricRegistry.scala"
cp "$here/ConsoleReporterExamples.scala" "$work/src/"

java -cp "$cp" scala.tools.nsc.Main -usejavacp -nowarn -d "$work/classes" "$work"/src/*.scala

main=org.apache.pekko.testkit.metrics.examples.ConsoleReporterExamples
# metrics-core logs through slf4j-api; without a binding it falls back to a
# no-op logger and says so on stderr, which is left as it is.
run_lf() { java -cp "$work/classes:$cp" "$main"; }
run_crlf() { java -Dline.separator=$'\r\n' -cp "$work/classes:$cp" "$main" crlf; }

if [ "${1:-}" = "--print" ]; then
  run_lf
  run_crlf
  exit 0
fi

run_lf >"$work/lf.out"
run_crlf >"$work/crlf.out"
sed '/^=== crlf: /,$d' "$here/console-reporter-examples.expected" >"$work/lf.expected"
sed -n '/^=== crlf: /,$p' "$here/console-reporter-examples.expected" >"$work/crlf.expected"
status=0
diff -u "$work/lf.expected" "$work/lf.out" || status=1
diff -u "$work/crlf.expected" "$work/crlf.out" || status=1
[ $status -eq 0 ] && echo "the output matches console-reporter-examples.expected"
exit $status
