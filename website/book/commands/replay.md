Only the transport changes. The catalogue is driven again through the same
request composition, the same response classification and the same assertion
evaluators the live run used, with the recorded response standing in for the
server's. A case whose recording runs out, or whose replay composes a request
the recording does not carry, records a transport failure: a verdict is never
reproduced over evidence nobody has.

With `--against`, every row is compared on its status and its two row counts,
and any disagreement exits `1` naming the case. The reason text is not
compared, because a replay reaches a recording rather than a server and
identical judgements can carry different words.

Omitting `--statement` re-derives a sweep of the whole catalogue. The replay
says so on stderr, in the words a live run uses, and stamps
`selection_basis: statement_blind` on the document it writes. With `--against`
the selection facts a `results.json` records are compared before any row is: a
record an ICS selected, re-judged blind, under a statement the record does not
name, or under one declaring different its-rest formats, exits `2` rather than
reporting agreement, because a re-derivation under another claim re-derives
another campaign. The statement is named by `statement_digest`, the leading 8
bytes of the SHA-256 over the declaration's own bytes, so the refusal prints
the recorded value and the applied one and a reader checks either with
`sha256sum statement.json | cut -c1-16`. A record written before
`selection_basis` or `statement_digest` existed identifies nothing about what
selected it, and the replay reports that instead of refusing.

What this establishes is that the judgement follows from the evidence. It does
not establish the evidence: a transcript is what the instrument says it sent
and received.
