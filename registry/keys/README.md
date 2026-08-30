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

## Provisioning it (owner action, once)

```bash
gpg --quick-generate-key 'Veredictum registry <registry@veredictum.eu>' ed25519 sign never
gpg --armor --export-secret-keys registry@veredictum.eu > registry-signing.sec.asc
gpg --armor --export             registry@veredictum.eu > registry/keys/registry-signing.pub.asc

gh secret set REGISTRY_SIGN_KEY --env registry-signing < registry-signing.sec.asc
# and REGISTRY_SIGN_PASSPHRASE the same way when the key carries one
shred -u registry-signing.sec.asc
```

Commit the public half. The `registry-signing` environment carries required
reviewers, so a run reaches the key only after a person lets it through, and
`.github/workflows/registry-console.yml` fails loudly rather than publishing an
unsigned entry when the secret is absent.
