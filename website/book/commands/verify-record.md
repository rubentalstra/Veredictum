Prints the signer fingerprint, the signing time, and one line per file with its
digest verdict. Zero findings is the only passing result: a digest mismatch, a
file the manifest names but the bundle does not carry, or a signature no
component of the supplied key verifies, each exits `1` naming what failed.

The bundle is ordinary files, so the check does not depend on this tool.
`gpg --verify record-manifest.json.asc record-manifest.json` establishes the
same signature, and `sha256sum` re-derives the same digests.

A verified bundle is one link in the chain and not the whole of it. A valid
signature proves integrity and origin since signing, and says nothing about the
conditions the run executed under. The published instrument, the verification
pack and the citation-carrying record are the rest, which is why that sentence
prints on every verification.
