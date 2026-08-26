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
| `comment-style.sh` | `.claude/rules/comments.md` — block comments, `TODO(#NNNN):` form, marker vocabulary, NOTE and comment-run budgets | `.claude/hooks/rust_fmt_clippy.sh` per edit; the `guards` job in `.github/workflows/ci.yml` |
| `changelog-structure.sh` | Keep a Changelog 1.1.0 — no duplicated `### <Type>` inside one release section, no header outside the canonical type set | the `guards` job in `.github/workflows/ci.yml` |
| `ci-conclusion-complete.sh` | branch protection routes through one `conclusion` check, so no CI job may run without appearing in its `needs` | the `workflow-audit` job in `.github/workflows/ci.yml` |

Two of these take a single file or no argument rather than the three modes
above: `changelog-structure.sh` reads one changelog and
`ci-conclusion-complete.sh` reads one workflow. The mode contract applies to
guards that scan a file set.

Guards still to port with the code: the spec-citation resolver, the
default-value style check, the typed-status check, and the SPDX header check.
They arrive with the migration
([FerroEHR#2789](https://github.com/rubentalstra/FerroEHR/issues/2789)), each
with the rule text that justifies it.
