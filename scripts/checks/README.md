# Repository guard scripts

Each script here enforces one rule from `.claude/rules/`. A rule without a
failing check is a wish, so a new rule lands together with its script.

Every script takes the same three modes, so the same code serves the per-edit
hook and the CI job:

```
scripts/checks/<name>.sh --all                 # the whole tree
scripts/checks/<name>.sh --diff <base> [head]  # changed files only
scripts/checks/<name>.sh --files <f>...        # named files (the edit hook)
```

Exit 0 is clean, 1 is violations listed as `file:line: message`, 2 is a usage
error.

| Script | Rule it enforces | Wired into |
|---|---|---|
| `comment-style.sh` | `.claude/rules/comments.md` — block comments, `TODO(#NNNN):` form, marker vocabulary, NOTE and comment-run budgets | `.claude/hooks/rust_fmt_clippy.sh` per edit; a CI job arrives with the CI lanes ([FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789)) |

Guards still to port with the code: the spec-citation resolver, the
default-value style check, the typed-status check, the SPDX header check, and
the changelog structure check. They arrive with the migration, each with the
rule text that justifies it.
