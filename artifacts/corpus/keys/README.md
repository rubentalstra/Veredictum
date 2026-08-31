# CNF version-signing test keys (OpenPGP, RFC 9580)

**Test-only key material** for the openPGP-posture half of every conformance
run. Not a secret: it exists so the composed SUT can sign VERSIONs with a known
OpenPGP key and the runner can verify the signatures against the paired public
key (RM common `master06-change_control_package.adoc` §Digital Signature — the
signature is generated per the openPGP standard). Never use these keys for
anything real.

The RM's own wording names IETF RFC 4880, and the catalogue's SIG-VERSION
cases quote it that way. Everything describing what this repository verifies
cites RFC 9580 instead: it obsoletes RFC 4880, and it is the revision the
pinned `pgp` crate implements.

- `cnf-signing.sec.asc` — the armored OpenPGP **secret** key (no passphrase).
  The signing half: whichever party runs the openPGP posture mounts it into
  their own server so it signs each committed VERSION. That deployment is the
  party's, not this repository's — FerroEHR mounts this file as
  `FERROEHR__SIGNING__KEY_PATH` in its own `docker/sut-signing-pgp.yml`, which
  lives in the FerroEHR tree and is named only in the topology text of the
  ixit fixture here.
- `cnf-signing.pub.asc` — the armored OpenPGP **public** key. Inlined into the
  `sut_pgp` instance's own `signing` block in the ixit fixture; the
  runner verifies `ORIGINAL_VERSION.signature` against it (RFC 9580 detached
  signature over the agreed canonical form: RFC 8785 JCS of the version minus
  `signature`).

Both halves are also the fixtures for this repository's own signing tests:
`app/veredictum/src/exec/signature.rs` round-trips a VERSION signature through
them, and `app/veredictum/src/record.rs` seals and verifies a run record with
them.

**Provenance.** A 3072-bit RSA primary key certifying a dedicated 3072-bit RSA
signing subkey (the in-OpenPGP certification chain — the OpenPGP analogue of a
CA→cert chain; openEHR version signing is OpenPGP, not X.509). The verifier
accepts a signature by either, since RFC 9580 §10.1 makes a transferable public
key the primary key plus its subkeys.

The committed certificate's own identity, read back from
`cnf-signing.pub.asc`, is:

```text
CNF Test Version Signing <cnf-signing@test.ehrbase-rs.local>
```

created 2026-07-24, no expiry. That user ID predates this repository, and the
recipe below reproduces it verbatim so a regeneration yields the same identity
rather than a second one. Re-cutting the pair under a new identity is a
deliberate change, not a cleanup: the public key is inlined into the ixit
fixture, so both would have to move together.

**Regeneration.** Read the identity back from the committed key first, so the
recipe cannot silently drift from it:

```bash
gpg --show-keys artifacts/corpus/keys/cnf-signing.pub.asc

export GNUPGHOME=/tmp/cg && mkdir -p "$GNUPGHOME" && chmod 700 "$GNUPGHOME"
gpg --batch --pinentry-mode loopback --gen-key <<'EOF'
%no-protection
Key-Type: RSA
Key-Length: 3072
Subkey-Type: RSA
Subkey-Length: 3072
Subkey-Usage: sign
Name-Real: CNF Test Version Signing
Name-Email: cnf-signing@test.ehrbase-rs.local
Expire-Date: 0
%commit
EOF
gpg --armor --export             cnf-signing@test.ehrbase-rs.local > cnf-signing.pub.asc
gpg --armor --export-secret-keys cnf-signing@test.ehrbase-rs.local > cnf-signing.sec.asc
```
