At least one of `--failing`, `--only` and `--filter` is required, and the three
union: a case is exported when any of them names it. The unfiltered document is
the transcript itself.

The red rows of a run become a triage input in one command:

```bash
veredictum evidence --transcript run/transcript.json \
    --results run/results.json --failing --out run/evidence.json
```

**No statement is read.** Sealing a record needs a claim; reading the exchanges
a run recorded does not, and a run that went red is exactly when they are
needed.

**An export that would carry nothing is refused**, exit `2`, with no file
written. A selection matching no recorded case names what was asked for and
what the transcript actually carries; a selection whose every case recorded no
exchange names those cases and says that recording is opt-in. A selection that
half-matched still exports, and the bundle's `without_exchanges` names every
case it could not carry, so a partial answer never reads as a complete one.

The `authorization` request header's value is withheld by the export itself,
whatever the transcript held. Response bodies are the wire's own bytes and can
carry real patient data, so the bundle is operator-controlled output like the
transcript it comes from.
