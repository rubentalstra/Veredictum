# Stacked pull requests (GitHub public preview)

GitHub ships native stacked pull requests: two or more PRs in one repository
where the bottom PR targets the trunk (`main` here) and each PR above targets
the branch of the PR below it. Each layer shows only its own diff, merges
bottom-up, and GitHub performs the cascading rebase when a lower layer merges.
Read 2026-08-30 from the official docs (durable citations at the bottom); the
feature is **public preview and subject to change**, so re-verify a command
against `gh stack --help` before leaning on it.

## Why this repository cares

The serial-PR workflow here has one recurring cost: every PR adds a bullet
under the changelog's Unreleased heading, so independent PRs go `DIRTY`
against each other and each merge forces a rebase-resolve-rerun cycle on
everything still open (the 2026-08-30 v0.1.2 merge train resolved the same
CHANGELOG conflict five times). A stack records the dependency once, rebases
in one cascade, and CI plus branch protection still gate every layer as if it
targeted `main`, so nothing about the required `conclusion` check weakens.

Use a stack when:

- one issue's fix builds on another's unmerged branch (the way #278's
  in-hand-envelope fix built on #263's `uid_pattern` branch);
- a large change decomposes into reviewable layers with real dependencies
  (schema first, machinery second, catalogue third);
- a batch of small fixes all touch the changelog and would otherwise conflict
  serially.

Do NOT use a stack to bundle unrelated changes: one issue per PR stays the
law, each layer still declares its own `Closes #<n>`, and each layer is still
a compiling, tested increment that passes the full gate battery on its own.

## The command surface (`gh stack`, extension)

Prerequisites: `gh` 2.90.0+, git 2.20+, `gh auth login`. Install once:

```shell
gh extension install github/gh-stack
```

The daily loop:

| Intent | Command |
|---|---|
| Start a stack (first branch, trunk selection) | `gh stack init` (`-b <trunk>`) |
| New layer on top | `gh stack add <branch>`, or `gh stack add -Am "<msg>"` (stage all + commit + branch) |
| Push every branch | `gh stack push` |
| Create/update the linked PRs | `gh stack submit` (`--auto` skips the editor; `--open` = ready-for-review, not draft) |
| Show the stack | `gh stack view` (`-s`, `--json`) |
| Fetch + cascading rebase + push + sync PR state | `gh stack sync` (`--prune` deletes local branches of merged PRs) |
| Cascading rebase only | `gh stack rebase` (`--downstack`, `--upstack`, `--continue`, `--abort`) |
| Restructure interactively (drop/fold/move/rename) | `gh stack modify` (`--continue`, `--abort`) |
| Adopt already-open PRs into a stack, no local tracking | `gh stack link <branch-or-pr> <branch-or-pr> …` |
| Merge one or more layers bottom-up | `gh stack merge [<pr>]` (`--squash`, `-y`) |
| Navigate | `gh stack up/down [n]`, `top`, `bottom`, `trunk`, `switch`, `checkout <pr>` |
| Stop tracking / unstack on GitHub | `gh stack unstack` (`--local` keeps the GitHub stack) |

`gh stack init` enables `git rerere`, so a conflict resolved once (the
changelog case) replays across the cascade. Exit code 9 means stacked PRs are
not enabled for the repository — the preview may need enabling before any of
this works here; check that BEFORE building a workflow on it.

## How the semantics interact with this repository's rules

- **Merging is bottom-up and merge-queue aware.** Merging a mid-stack PR
  merges everything below it; the PRs above re-target `main` automatically
  and GitHub runs the cascading rebase server-side. Squash merge (this
  repository's method) is supported, and the resulting history equals merging
  each PR individually from the bottom.
- **Branch protection and CI run per layer** as if each PR targeted the
  bottom's base branch, so the required `conclusion` check, the guards, and
  CODEOWNERS gate every layer. A green mid-stack layer is a real green.
- **One `Closes #<n>` per layer.** The tracker linkage does not change:
  each layer closes its own issue when it merges.
- **Signed commits still apply** (hard rule 9): `gh stack` drives local git,
  so `commit.gpgsign=true` covers every layer's commits. Never let a rebase
  strip identity.
- **The changelog entry rides its own layer.** With `git rerere` on, the
  recurring Unreleased-section conflict is resolved once per stack instead of
  once per merge.
- **Same-repository only.** Cross-fork stacks are unsupported, so an outside
  contributor's PR can never be a stack layer here.
- **Hand-stacked PRs are not stacks.** A PR whose base is another PR's head
  branch (the pre-feature idiom) gets none of the cascading-rebase or
  retargeting behaviour; after the base squash-merges it needs a manual
  `git rebase --onto origin/main <old-base>`. Use `gh stack link` to adopt
  such PRs into a real stack instead.
- **API surface exists** (REST list/create/extend/dissolve, read-only GraphQL
  `stack` fields, a `stack` object in `pull_request` webhooks), and merging
  via API needs the asynchronous stack merge endpoint. Only relevant if a
  workflow ever automates stack merges; none does today.

## Adoption status

Not adopted yet. This file is the reference so adoption is a decision, not a
rediscovery. Before first use: verify the repository has the preview enabled
(exit code 9 check above), run one throwaway two-layer stack end to end
(init → add → submit → merge bottom → confirm the top re-targets and the
cascade rebases), and record the outcome on a tracker issue. Until then the
serial train (rebase → green → squash-merge, one PR at a time) remains the
working procedure.

## Official documentation (durable citations)

- About stacked pull requests —
  <https://docs.github.com/en/pull-requests/get-started/about-stacked-prs>
- Quickstart —
  <https://docs.github.com/en/pull-requests/get-started/stacked-prs-quickstart>
- CLI command reference —
  <https://docs.github.com/en/pull-requests/reference/stacked-prs-cli-commands>
