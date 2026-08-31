<!--
SPDX-FileCopyrightText: Veredictum contributors
SPDX-License-Identifier: Apache-2.0
-->

# The registry signing key

One key exists in this project, and this directory holds its public half.

It signs the record of a **console** entry: a run performed at
console.veredictum.eu, whose verdicts this repository's CI re-derived from the
submitted transcript before signing anything. `registry/RULES.md` states what
that tier attests and what it cannot.

`registry-signing.pub.asc` is what a reader checks a published record against,
with the instrument or without it:

```bash
veredictum verify-record \
  --record registry/records/<system>/<entry-id> \
  --key registry/keys/registry-signing.pub.asc

gpg --verify record-manifest.json.asc record-manifest.json
```

## Where the secret half lives, and where it does not

In the `registry-signing` GitHub environment, and nowhere else. Not in this
tree, not on the hosted instrument, and not in any workflow a pull request can
influence. `scripts/checks/registry-submission.sh` refuses any armored private
key committed under `registry/`, and the environment's protection rules are
what stand between a workflow run and the key.

The tier-1 reproduction lane holds no key at all: its evidence is the workflow
identity itself through Sigstore, so there is nothing there to steal. The
console tier needs a signature because the instrument that produces its records
runs on a public host that must hold no key, which means the signature has to
be made somewhere that host cannot reach.

## The key that exists

```text
pub   ed25519 2026-08-31 [C]  E9B93C1F004B1C26F5ADD67DC45D64872EE0C7A1
uid                           Veredictum registry <registry@veredictum.eu>
sub   ed25519 2026-08-31 [S]  612AA45AA5E1948E1597EF5D121B2FD07350433B
```

The primary key only certifies; the subkey signs. So a verified record names the
**subkey** fingerprint as its signer, and a reader comparing it against the
primary above has not found a wrong key. It carries no expiry and no passphrase:
a passphrase would live in the same environment as the key it unlocks, which is
no second boundary, and the environment's required reviewers are the boundary
that exists.

## How it was provisioned (owner action, once)

The secret half was generated in a throwaway `GNUPGHOME` and never entered a
personal keyring, so the environment secret is the only copy that survives.

```bash
export GNUPGHOME="$(mktemp -d)"; chmod 700 "$GNUPGHOME"
gpg --batch --passphrase '' --quick-generate-key \
    'Veredictum registry <registry@veredictum.eu>' ed25519 cert never
gpg --batch --passphrase '' --quick-add-key <FPR> ed25519 sign never

gpg --armor --export-secret-keys registry@veredictum.eu > "$TMPDIR/sec.asc"
gpg --armor --export             registry@veredictum.eu \
    > registry/keys/registry-signing.pub.asc

gh secret set REGISTRY_SIGN_KEY --env registry-signing < "$TMPDIR/sec.asc"
```

Then the exported secret, the temporary keyring and its agent were destroyed,
and the revocation certificate gpg writes at creation was moved out of the
keyring before it went — it is the only way to retire this key, because nothing
can read the environment secret back.

The `registry-signing` environment carries **required reviewers**, so a run
reaches the key only after a person lets it through, and a deployment branch
policy naming `main` alone, so no other ref can request it.
`.github/workflows/registry-console.yml` fails loudly rather than publishing an
unsigned entry when the secret is absent.

## What was verified before the key was used

The engine's own signing and verification code (`app/veredictum/src/record.rs`),
compiled against the same `pgp` pin, was run over this exact key material: it
parses the gpg export, finds the signing subkey, produces a detached signature,
accepts it against the committed public half, and refuses it against a modified
body. The check matters because the committed test keypair is RSA, so an EdDSA
key would otherwise have reached production on a path nothing here had exercised.
