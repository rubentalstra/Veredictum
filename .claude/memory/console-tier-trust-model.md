---
name: console-tier-trust-model
description: Why a console record is trustworthy without trusting the console — the transport seam, the re-derivation gate, and the two lessons that gate taught
metadata:
  type: project
---

A record produced at console.veredictum.eu is trustworthy because its judgement
is recomputed here, never because the instance is trusted. Two mechanisms carry
that, and both were built in v0.1.4 (#392, #408).

**The transport is the driver's only seam.** `HttpDriver` composes every
request, classifies every response and judges every assertion; `Transport` is
the one place bytes leave the process. So `veredictum replay` answers a composed
request from a recording and reaches its outcomes through the same code the live
run used — a second reading of the evidence rather than a second implementation
of the judge. Changing that boundary breaks the property the console tier rests
on.

**Two lessons the gate taught, the expensive way.** A `console` submission
carries NO provenance block, because the instrument may not state its own
provenance and the lane writes it afterwards — so any code that decides what to
do by reading `.provenance.tier` skips every real submission. And the test that
should have caught that asserted the gate's exit status was success: a skip
exits zero exactly as a clean re-derivation does.

**Why:** a gate is proven by refusing a bad submission, never by staying quiet
about a good one.

**How to apply:** when you add or change a gate, assert that it RAN — a count,
a named row, a refusal — and drive the bad input through it in the same test.
See [[hosted-instrument-box]] for where the instrument runs.
