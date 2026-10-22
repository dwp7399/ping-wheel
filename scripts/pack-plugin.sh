#!/usr/bin/env bash
# 把插件目录打成可安装的 zip：scripts/pack-plugin.sh examples/plugins/translate [输出目录]
set -euo pipefail

dir="${1:?用法：pack-plugin.sh <插件目录> [输出目录]}"
out="${2:-dist/plugins}"
[ -f "$dir/manifest.json" ] || { echo "错误：$dir 下没有 manifest.json" >&2; exit 1; }

id=$(python3 -c 'import json,sys; m=json.load(open(sys.argv[1])); print(m["id"]+"-"+m["version"])' "$dir/manifest.json")
mkdir -p "$out"
zip_path="$(cd "$out" && pwd)/$id.zip"
rm -f "$zip_path"
(cd "$dir" && zip -qr -X "$zip_path" . -x '.*')
echo "已打包：$zip_path"
