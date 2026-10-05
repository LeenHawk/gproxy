#!/usr/bin/env bash
# Post-write checks for the docs site: sidebar slugs vs pages, locale parity,
# forbidden references, oversized pages.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
root=docs/src/content/docs
status=0
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "== sidebar slugs without a page =="
while read -r slug; do
  for locale in "" zh-cn/ zh-tw/; do
    [ -f "$root/$locale$slug.md" ] || [ -f "$root/$locale$slug.mdx" ] || {
      echo "  ${locale:-EN/}missing: $slug"; status=1;
    }
  done
done < <(grep -oE "slug: '[a-z0-9/-]+'" docs/astro.config.mjs | sed "s/slug: '//; s/'//" | sort -u)

echo "== locale page parity =="
(cd "$root" && find . \( -path ./zh-cn -o -path ./zh-tw \) -prune -o -type f -print | sed 's|^\./||' | sort) > "$work/en.txt"
for locale in zh-cn zh-tw; do
  (cd "$root/$locale" && find . -type f -print | sed 's|^\./||' | sort) > "$work/$locale.txt"
  diff "$work/en.txt" "$work/$locale.txt" | sed "s|^|  $locale: |" || status=1
done

echo "== forbidden references =="
grep -rnE 'AGENTS\.md|CLAUDE\.md|design/[a-z-]+\.md|releases/latest/download/gproxy-|GPROXY_SECRET_KEY|crates\.io/crates/gproxy' "$root" | sed 's/^/  /' && status=1

echo "== frontmatter present =="
for f in $(find "$root" -type f); do head -1 "$f" | grep -q '^---$' || { echo "  no frontmatter: $f"; status=1; }; done

echo "== pages over 320 lines =="
find "$root" -type f -exec wc -l {} + | sort -rn | awk '$1 > 320 && $2 != "total" {print "  " $0}'

echo "== totals =="
echo "  EN: $(wc -l < "$work/en.txt") files, zh-cn: $(wc -l < "$work/zh-cn.txt") files, zh-tw: $(wc -l < "$work/zh-tw.txt") files"
exit $status
