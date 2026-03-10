#!/usr/bin/env bash
set -euo pipefail

# Regenerate THIRD_PARTY_NOTICES.md from Cargo metadata.
# Run from repository root:
#   ./scripts/generate-third-party-notices.sh

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is required" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required" >&2
  exit 1
fi

out_file="THIRD_PARTY_NOTICES.md"
tmp_json="$(mktemp)"
trap 'rm -f "$tmp_json"' EXIT

cargo metadata --format-version 1 --locked > "$tmp_json"

{
  cat <<'EOF'
# Third-Party Notices

This project depends on open-source Rust crates.
This compact list is grouped by upstream project/repository (one row per project).
Generated from `cargo metadata --locked`.

| Project | Crates | Versions | Licenses | Authors |
|---|---|---|---|---|
EOF

  jq -r '
    .packages
    | map(select(.source != null))
    | map({
        project: (.repository // .homepage // ("crate:" + .name)),
        crate: .name,
        version: .version,
        license: (.license // "UNKNOWN"),
        authors: (.authors // [])
      })
    | group_by(.project)
    | map({
        project: .[0].project,
        crates: ([.[].crate] | unique | sort),
        versions: ([.[].version] | unique | sort),
        licenses: ([.[].license] | unique | sort),
        authors: ([.[].authors[]?] | unique | sort)
      })
    | sort_by(.project)
    | .[]
    | [
        .project,
        (.crates | join(", ")),
        (.versions | join(", ")),
        (.licenses | join("; ")),
        (if (.authors|length)==0 then "(not specified)" else (.authors|join("; ")) end)
      ]
    | @tsv
  ' "$tmp_json" | while IFS=$'\t' read -r project crates versions licenses authors; do
    project=${project//|/\\|}
    crates=${crates//|/\\|}
    versions=${versions//|/\\|}
    licenses=${licenses//|/\\|}
    authors=${authors//|/\\|}
    printf '| %s | %s | `%s` | %s | %s |\n' \
      "$project" "$crates" "$versions" "$licenses" "$authors"
  done

  cat <<'EOF'

## Notes

- Author and license fields are taken from each crate's published Cargo metadata.
- Some crates intentionally omit author fields.
- For full license texts, refer to each crate's source package or repository.
EOF
} > "$out_file"

echo "Updated $out_file"
