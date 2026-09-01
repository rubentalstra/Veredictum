A pack is versioned data compiled into the binary, so the binary is the only
honest source for a description of one. This command writes that description:
per pack the id, the version, the seed every arrival stream draws from, the
failed-arrival ceiling a record is judged against, each phase with its load
discipline and its counts, each measured phase's operation
mix with the share and the probe rationale of every entry, each posture profile
the pack defines with what it declares item by item, and each embedded fixture
with its sha256 pin, its size and where the bytes came from. The document also
carries the boundary statement, the methodology, how a relative index is
derived, what the seed and the posture canaries govern, and the requirements a
record meets before it may be ranked.

Emission is byte-deterministic and every collection is ordered, so regenerating
the file and diffing it is a build gate. The public page at
[veredictum.eu/benchmark-methodology.html](https://veredictum.eu/benchmark-methodology.html)
is generated from the committed copy of this document, and CI refuses a pack
change that leaves either of them stale.
