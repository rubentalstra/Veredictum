# VEX statements

Vulnerability Exploitability eXchange documents, in [OpenVEX](https://openvex.dev)
format, asserting the exploitability of specific findings in the image this
project publishes and in its Rust dependency graph.

Ported from FerroEHR's `security/vex/` at the console scaffold (#53), with the
same standing rules. A VEX document is not a way to silence a scanner. It is
the machine-readable form of an argument, and it carries the argument with it:
each statement names the vulnerability, the product, a `status`, a
controlled-vocabulary `justification`, and an `impact_statement` a reader can
check. The alternative, an ignore list, records the decision without the
reasoning, which decays into a list nobody can re-evaluate.

## Rules

- **`not_affected` needs a justification from the OpenVEX vocabulary**, and the
  `impact_statement` must say concretely why the code is unreachable in *our*
  usage. "Low risk" is not a justification.
- **A finding we can fix is fixed, not VEXed.** These documents exist for
  findings in inherited upstream layers and for adjudicated Rust advisories,
  where the fix belongs to someone else.
- **Re-check on every base-image bump.** When an upstream image rebuilds its
  bundled bytes, statements about them become obsolete and the entries go — a
  stale `not_affected` is worse than no VEX at all.
- The scanners consume these files (`trivy --vex`), so a statement that stops
  being true stops being invisible: the finding returns and the gate fails.
- The container-layer statements pair with `.trivyignore.yaml`: the ignore
  file is what the shared `trivy.yaml` wires into every lane and carries the
  expiry date; the OpenVEX document is the published, machine-readable form of
  the same adjudication. The two are edited in the same change, always.

## Documents

| File | Subject | Authored |
|---|---|---|
| `distroless-libssl.openvex.json` | The OpenSSL finding in the distroless base filesystem. Not affected: nothing in the shipped image links libssl. | by hand |
| `rust-advisories.openvex.json` | The Rust dependency advisories `deny.toml` accepts with an adjudication. | **generated** |

## The generated document

`rust-advisories.openvex.json` is produced by
`scripts/security/vex-generate.sh` from two inputs, and must never be edited
by hand:

- **`deny.toml`** `[advisories].ignore` — the authoritative set of advisory
  ids. It is the gate that actually decides whether a build passes, so it is
  the only place the id list may live.
- **`security/vex/rust-advisories.toml`** — the reasoning: the OpenVEX
  `status`, the controlled-vocabulary `justification`, and the
  `impact_statement` for each id. Unmaintained-class notices deny.toml
  accepts sit in its `[[informational]]` tier: they are not vulnerabilities,
  so no OpenVEX statement is emitted for them, and the generator still holds
  them to the same two-way agreement with the gate.

Two lists that must agree is a shape that drifts silently, so the generator
refuses to emit anything unless the two sets match in **both** directions, and
`scripts/checks/vex-advisories.sh` (run in the CI guard tier) regenerates the
document and fails on any difference. Adding an ignore to `deny.toml` without
publishing its justification is a red build, not an oversight nobody notices.

Agreement is not the same as truth: an ignore and its justification stay in
perfect agreement while a dependency upgrade quietly resolves the advisory
underneath both. `deny.toml`'s own rule closes that half — every ignore names
its DELETING EVENT and is re-checked at every dependency-bump cycle.
