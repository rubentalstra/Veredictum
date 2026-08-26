<!-- Describe the change itself. No AI or tool attribution anywhere in this PR. -->

## What this changes

Closes #<!-- tracker issue number; the merge closes it. One Closes keyword per issue. -->

## Specification citations

<!--
For anything that changes an expectation, a verdict, or wire behaviour: the
component, the document, and the section heading, with the sentence you are
relying on quoted. Where the released specification text and the OpenAPI
documents are both silent, say so and name the register disposition.
An expectation with no citation is not reviewable.
-->

## Checks

- [ ] `bash scripts/checks/comment-style.sh --all`
- [ ] Rust gates green: `cargo fmt --all --check` · `cargo clippy --all-targets -- -D warnings` · `cargo nextest run` · `cargo deny check`
- [ ] `cargo run -- validate --root artifacts --specs specs/openehr` reports zero findings
- [ ] No test, case, or expectation weakened, skipped, or deleted, and no test edited to route around a defect it exposes
- [ ] Every new or changed expectation carries the specification section it comes from
- [ ] No expectation set or adjusted from what a server under test did
- [ ] User-visible change → `CHANGELOG.md` `[Unreleased]` entry
- [ ] Commits signed (`git log --format=%G?` reads `G`)
- [ ] New issues from this work linked as native GitHub relationships (sub-issue, blocked-by), not prose

<!--
HARD RULE: this description, the title, and every commit contain NO AI or tool
attribution. No "Co-Authored-By" trailer of any kind, no "Generated with", no
robot emoji. Describe only the change. If an AI tool was used, disclose it in
this description per AI_STATEMENT.md § 10.
-->
