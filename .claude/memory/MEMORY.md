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
- [Migration state](migration-state.md) — the code lives here now; what the
  extraction carried, what it re-rooted, and what is still open on the consumer
  side
- [Contents API commits are unsigned](contents-api-commits-unsigned.md) —
  learned 2026-08-26: never write to the repository through the GitHub contents
  API; local signed commits or the Git Data API in workflows
- [Release conventions](release-conventions.md) — bare-version titles; immutable
  releases, so the pipeline publishes the draft last; crates.io Trusted
  Publishing matches `workflow_ref`, which names the CALLING workflow, so the
  shared publish logic is a script and each entry point needs its own publisher
  entry; the Zenodo concept DOI
