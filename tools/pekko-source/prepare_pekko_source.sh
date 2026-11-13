#!/usr/bin/env bash
# 作業ツリー直下に Pekko v1.7.1 の原典 pekko/ を用意する。
# TAKT の workflow_config.runtime.prepare から、作業ツリーを cwd にして実行される
# （init と各ステップの前）。stdout は TAKT が KEY=VALUE として読むため、何も出さない。
set -euo pipefail

# TAKT は親の環境をそのまま渡すため、git hook から起動されると GIT_DIR などで
# repo の場所が差し替えられ、pekko/ ではなく呼び出し元の repo を照合してしまう。
# repo の場所を指す変数だけを消す。GIT_CONFIG* と GIT_ALLOW_PROTOCOL は設定の
# 受け渡しに使うため残す。
unset GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_OBJECT_DIRECTORY GIT_DIR GIT_WORK_TREE \
  GIT_IMPLICIT_WORK_TREE GIT_GRAFT_FILE GIT_INDEX_FILE GIT_NO_REPLACE_OBJECTS \
  GIT_REPLACE_REF_BASE GIT_PREFIX GIT_SHALLOW_FILE GIT_COMMON_DIR GIT_NAMESPACE

# TAKT の runtime.prepare サンドボックスは XDG_CONFIG_HOME を作業ツリー内の空ディレクトリへ
# 向ける。run 用 opencode サーバは provider 定義を利用者設定から解決するため、graft しないと
# モデル解決が「Model unavailable」で失敗する。利用者設定が存在し、サンドボックス側の
# opencode.json が無いか既に symlink のときだけ graft する（冪等、stdout へは出さない）。
oc_user_cfg="${HOME:-}/.config/opencode/opencode.json"
oc_sb_dir="${XDG_CONFIG_HOME:-$PWD/.takt/.runtime/config}/opencode"
if [ -f "$oc_user_cfg" ] && { [ ! -e "$oc_sb_dir/opencode.json" ] || [ -L "$oc_sb_dir/opencode.json" ]; }; then
  mkdir -p "$oc_sb_dir"
  ln -sfn "$oc_user_cfg" "$oc_sb_dir/opencode.json"
fi

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
  # ここへ来る clone はこの実行で prepare が作ったものだけ。残すと、原典を直して
  # 再実行しても既存 repo として拒否され、自動準備に戻れなくなる。
  rm -rf "$target"
  exit 1
fi
