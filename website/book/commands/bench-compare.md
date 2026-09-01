One column per file, one row per phase, operation and metric. Each cell carries
the cross-repetition median with the inter-quartile range beside it, so a reader
sees the spread as well as the number.

Every column header carries the machine the run was generated on, the worst
failed-arrival share the run recorded beside the ceiling its pack pins, and a
column that is not submittable names the requirements it misses rather than
printing one bare `false`. Where the columns carry a relative index, it gets its
own table below the header, at `p50` and `p99` per baseline: that is the part
which survives a change of host.

A mismatch is stated above the table, never under it: columns that ran different
pack versions, columns generated from different hosts, columns that ran at
different scale factors or off the pack's pinned configuration, and columns whose
runs are not submittable are all named in the header, and the command exits `1`
when any of them applies. Columns from different hosts where at least one
carries no relative index are named too, because nothing in that table is
comparable across them. Each row also names the discipline its numbers came
from.
