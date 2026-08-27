# Maintainers and access continuity

This file is the roster and the honest answer to the question anyone relying on
a published verdict should ask: *who can put bytes in front of me under this
name, and what happens if they are unavailable?*

It is deliberately not aspirational. Everything below describes the project as
it is on the day you read it in git history.

## Roster

| Person        | GitHub                                           | Role              | Since      |
|---------------|--------------------------------------------------|-------------------|------------|
| Ruben Talstra | [@rubentalstra](https://github.com/rubentalstra) | Maintainer (sole) | 2026-08-26 |

**The bus factor of this project is one.** There is exactly one person with
write access (`GET /repos/rubentalstra/Veredictum/collaborators` returns one
login), one person who can cut a release, and one person who can accept a pull
request. No second maintainer exists, no organisation stands behind the project,
and no legal entity is a party to it.

The same person maintains [FerroEHR](https://github.com/rubentalstra/FerroEHR),
one of the CDRs this instrument grades. That conflict of interest, and the
mechanical constraints on it, are stated in
[GOVERNANCE.md § The specifications decide conformance](GOVERNANCE.md#the-specifications-decide-conformance-not-the-maintainer).
The route to a second maintainer, including one from a competing
implementation, is in the same file and it is open.

## Publishing identities and where they live

These are the credentials and configured identities that can publish something
under the Veredictum name. Naming them is the point: an inventory nobody has
written down is an inventory nobody can hand over.

| Identity | What it publishes | State today | Held by | Recovery if the holder is unavailable |
|---|---|---|---|---|
| The GitHub account `rubentalstra` | everything: the repository, releases, issues, settings, labels | live | the maintainer | none. The repository is user-owned, so GitHub's account-recovery process is the only route, and it is between GitHub and the account holder |
| The OpenPGP commit- and tag-signing key | the verified signature on every commit and every release tag | live | the maintainer, on his own hardware | none. The private key is not escrowed. A successor would publish a new key and re-establish trust from a signed statement on the repository; historical signatures stay verifiable regardless |
| `GITHUB_TOKEN` (ephemeral, per workflow run) | the GitHub release and the GHCR container image (`ghcr.io/rubentalstra/veredictum`) | live — the release pipeline (#12) has cut three alphas with it | GitHub, minted per run; nothing is stored | not applicable. There is no credential to lose |
| Zenodo | the archived release deposit and its concept DOI (10.5281/zenodo.22113258) | live — connected and proven at v0.0.1-alpha.1 (version DOI 10.5281/zenodo.22113259) | the Zenodo account linked to the GitHub account | tied to GitHub account recovery |
| crates.io | the [`veredictum`](https://crates.io/crates/veredictum) crate | live — publishes via Trusted Publishing (OIDC, `publish-crates.yml`, the `crates-io` environment); no stored token | the crates.io account linked to the GitHub account, plus the per-workflow Trusted Publisher configuration | tied to GitHub account recovery; the Trusted Publisher config is re-creatable by any crate owner |
| The `veredictum.eu` domain | the landing page and the documentation site (GitHub Pages, #33) | live | the maintainer's DNS registrar account | none beyond the registrar's own recovery; the Pages site itself follows the repository |

**The honest reading of that table:** every identity that exists today
terminates at one person's GitHub account, one person's hardware, or one
person's registrar login. Nothing here distributes authority, and no
mitigation is available to a one-person project without a legal entity behind
it. The publishing paths themselves store no secret — the release lane and
the crate publish both run on per-run tokens (ephemeral `GITHUB_TOKEN`,
crates.io Trusted Publishing OIDC) — which removes the stored-secret risk
without changing that sentence.

## If the maintainer is unavailable

There is no succession plan a document can create. What exists instead:

- **Nothing already published disappears.** Once the release lane exists,
  releases are immutable and their assets stay downloadable; a published
  container digest is not withdrawable; a Zenodo DOI is permanent. A pinned
  version already in someone's pipeline keeps working.
- **Nothing new ships.** No release, no new case, no fix to a mis-judged row.
- **The work is not lost, and the licence is the mechanism.** Apache-2.0, public
  history, every rule a committed file, every expectation carrying its own
  specification citation. A fork is a complete and legitimate continuation of
  this instrument, and the project's position is that it should be taken rather
  than waited on. The citations are what make that possible: a fork inherits a
  catalogue whose correctness can be re-derived from the openEHR specifications
  without asking anyone here.
- **A vulnerability report has a fallback.** If a private report receives no
  acknowledgement within the window [SECURITY.md](SECURITY.md) commits to, that
  policy already tells you to escalate publicly. That path does not depend on
  the maintainer.

If you plan to rely on this instrument for a published conformance claim and
that position is not acceptable to you, the mitigation is on your side: pin a
version, keep a fork you can build, and record which version produced the
verdict you published.

## Adding a maintainer

The route is in [GOVERNANCE.md](GOVERNANCE.md). When someone takes it, this file
gains a row, [`.github/CODEOWNERS`](.github/CODEOWNERS) gains their handle on
the areas they own, and the table above gains a second holder wherever the
identity permits one. Those three edits are the whole mechanism.
