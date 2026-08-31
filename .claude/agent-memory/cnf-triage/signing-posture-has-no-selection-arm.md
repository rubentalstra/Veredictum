---
name: signing-posture-has-no-selection-arm
description: "signature: verifiable requested but the ixit declares no `signing` posture" is a runner gap — every other missing ixit fact resolves not-applicable at selection time, this one errors at assert time
metadata:
  type: project
---

A red row reading `signature: verifiable requested but the ixit declares no
`signing` posture` is RUNNER-side: the case should have been selected away as
not-applicable, not driven and then reported unjudgeable.

**Why:** signing is optional in the release. RM common
`master06-change_control_package.adoc` §Digital Signature: "At the time of
committal of a Version, a digital signature of the object **can** be made … If
public key or equivalent infrastructure is in place so that users are able to
sign content, a digital signature can be created" and "The signature, **if
present**, is generated according to the openPGP standard". So the mode is a
deployment fact, exactly as `app/veredictum/src/exec/driver.rs` says at the
refusal. But `run.rs` `selection_exception` has arms for `smart`, `system_id`,
`dump_location`, terminology, spec profile, administrative posture and missing
instances — and NONE for `signing`, so the row errors after mutating the SUT
instead of resolving n-a like its siblings.

**How to apply:** the discriminator is which assertion the case makes. A
`signature` assertion of PRESENCE or verbatim storage passes with no posture
declared; only `verifiable: true` needs the mode. Check the `-pgp` sibling too —
it resolves n-a on the missing `sut_pgp` instance, which is the precedent the
digest sibling lacks.
