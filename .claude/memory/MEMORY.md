# Memory index

Persistent agent memory, kept in-repo so it is visible and versioned. The
harness memory directory under `~/.claude/projects/<project-slug>/memory` is a
symlink to this directory; never break that link, and never move memory out of
the repository.

Each file carries frontmatter (`name`, `description`, `metadata.type`) and one
durable fact, decision, or lesson. Keep them short, and write the "how to
apply" part so a future session can act on it without re-deriving it.

- [Product identity and origin](product-identity-and-origin.md) — the name
  Veredictum, why the instrument is a separate product, the rejected
  alternatives, and the FerroEHR#2789 pointer
- [The move out of FerroEHR](migration-state.md) — history, finished on
  2026-08-26: the unsigned carried history, the separately vendored ITS and OAS
  bundles, and the lint configuration that was adapted rather than copied
- [Contents API commits are unsigned](contents-api-commits-unsigned.md) —
  learned 2026-08-26: never write to the repository through the GitHub contents
  API; local signed commits or the Git Data API in workflows
- [Release conventions](release-conventions.md) — bare-version titles; immutable
  releases, so the pipeline publishes the draft last; crates.io Trusted
  Publishing matches `workflow_ref`, which names the CALLING workflow, so the
  shared publish logic is a script and each entry point needs its own publisher
  entry; the Zenodo concept DOI
- [Console UI direction](console-ui-direction.md) — FerroEHR shell + Veredictum
  palette, everything-in-the-UI, signed verifiable records; design record #61,
  engine signing #62
- [Subagent worktree isolation](subagent-worktree-isolation.md) — parallel implementers need isolation:worktree; shared checkout mixes authorship; REST over GraphQL when agents saturate gh
- [SonarCloud API workflow](sonarcloud-api-workflow.md) — token env, accept/falsepositive transitions with comments, idempotent bulk loops
- [Use PR stacks](use-pr-stacks.md) — owner ruling 2026-08-30: dependent or
  batched changes go through GitHub stacked PRs after the adoption gate in
  `.claude/rules/stacked-prs.md`; the serial changelog merge train is the
  failure mode to avoid
- [Commit memory promptly](commit-memory-promptly.md) — memory files never
  float in the working tree; sweep and push them with the work that produced
  them
- [Drafting in Ruben's voice](drafting-in-rubens-voice.md) — personal drafts
  (emails, forum posts) lead with the point, short paragraphs, no structure
  announcements, no AI tells; Discourse is markdown, Outlook is .txt
- [Hosted console on Vercel](hosted-console-vercel.md) — console.veredictum.eu, view-only by construction (no engine in the image), deploy-hook only, FerroEHR#2945 service-root lesson
