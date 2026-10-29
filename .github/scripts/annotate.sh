#!/usr/bin/env bash
# 失敗したログの要点を GitHub の注釈（::error）として出す。
# ログ本文を開けない環境からでも、API の注釈で原因が読めるようにするため。
log="$1"
[ -f "$log" ] || exit 0
emit() {
  local title="$1"; shift
  local body
  body=$(cat | head -c 60000 | sed -e 's/%/%25/g' -e 's/\r//g' | awk 'BEGIN{ORS="%0A"} {print}')
  [ -n "$body" ] && echo "::error title=${title}::${body}"
}
# コンパイルエラー
grep -n -A12 -E "^error(\[E[0-9]+\])?:" "$log" | head -150 | emit "コンパイルエラー"
# テストの失敗
grep -n -B2 -A25 -E "^---- .* stdout ----|panicked at" "$log" | head -150 | emit "テストの失敗"
# 最後の 60 行
tail -60 "$log" | emit "ログの末尾"
