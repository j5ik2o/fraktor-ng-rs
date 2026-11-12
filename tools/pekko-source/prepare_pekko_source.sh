#!/usr/bin/env bash
# 作業ツリー直下に Pekko v1.7.1 の原典 pekko/ を用意する。
# TAKT の workflow_config.runtime.prepare から、作業ツリーを cwd にして実行される
# （init と各ステップの前）。stdout は TAKT が KEY=VALUE として読むため、何も出さない。
set -euo pipefail

target="$PWD/pekko"
local_repo="${PEKKO_SOURCE_LOCAL_REPO:-/Users/j5ik2o/Sources/github.com/j5ik2o/pekko}"
upstream_url="${PEKKO_SOURCE_UPSTREAM_URL:-https://github.com/apache/pekko}"
expected="${PEKKO_SOURCE_EXPECTED_COMMIT:-9a54d1294c3ac7821bbed3c940769c3d5027d1ff}"
tag="v1.7.1"

prepare() {
  rm -rf "$target"
  if [ -d "$local_repo" ] && git -C "$local_repo" rev-parse --git-dir >/dev/null 2>&1; then
    if git clone --branch "$tag" "$local_repo" "$target" >&2; then
      return 0
    fi
    rm -rf "$target"
  fi
  git clone --depth 1 --branch "$tag" "$upstream_url" "$target" >&2
}

if [ -e "$target" ] || [ -L "$target" ]; then
  # 作業ツリー自体が git repo なので、空の pekko/ で rev-parse HEAD を実行すると
  # 外側の HEAD が返る。pekko/ 自身が git の toplevel であることを先に確かめる。
  if [ -d "$target" ] && toplevel=$(git -C "$target" rev-parse --show-toplevel 2>/dev/null) \
     && [ "$(cd "$toplevel" && pwd -P)" = "$(cd "$target" && pwd -P)" ] \
     && head=$(git -C "$target" rev-parse --verify HEAD^{commit} 2>/dev/null); then
    if [ "$head" = "$expected" ]; then
      exit 0
    fi
    echo "pekko/: HEAD is $head, expected $expected; not removing it" >&2
    exit 1
  fi
  echo "pekko/ is incomplete; rebuilding" >&2
  prepare
else
  prepare
fi

head=$(git -C "$target" rev-parse --verify HEAD^{commit})
if [ "$head" != "$expected" ]; then
  echo "pekko/: HEAD is $head, expected $expected" >&2
  exit 1
fi
