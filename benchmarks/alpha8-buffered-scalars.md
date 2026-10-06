# A8 buffered scalar measurements

Compare the published A7 binary with the bounded scalar candidate described in
[ADR 0077](../docs/decisions/0077-buffered-scalar-cells.md). Reports retain exact
source patches, binary hashes, fixture hashes and individual observations:
[numeric](results/alpha8-buffered-scalars-numeric.json) and
[text/styles](results/alpha8-buffered-scalars-text-styles.json).

Release samples ran serially without overlapping builds, fixture generation or
profiling: one warmup and three rotating repetitions per mode and scale. Time
includes decoding and value checks; full-model modes retain all decoded cells.
Calamine 0.36.1 produces noneditable ranges; overlapping value-read behavior is
compared without claiming equivalent editing or preservation. Native wait4 RSS
and sampled temporary storage are reported separately from logical allowances.

| Workload | A7 seconds | Candidate seconds | Calamine seconds |
| --- | ---: | ---: | ---: |
| Two million numeric cells, stream | 0.933477 | 0.348560 | 0.428452 |
| Two million numeric cells, retained model | 1.092915 | 0.480808 | 0.524424 |
| Two million numeric cells, read/edit/save | 3.640193 | 2.900161 | not compared |
| One million repeated shared-text cells, RAM SST model | 0.695350 | 0.365583 | 0.419005 |
| One million unique shared-text cells, RAM SST model | 1.324171 | 0.975127 | 0.742023 |
| One million styled numeric cells, model | 0.736433 | 0.526795 | not compared |

Numeric model median peak RSS is 69,148 KiB versus calamine's 105,960 KiB.
Numeric stream RSS is 5,148 KiB versus 4,636 KiB, so that RAM target remains
unmet. Repeated-text model RSS is 68,764 KiB versus 160,424 KiB; unique-text
model RSS is 224,988 KiB versus 308,960 KiB, while its elapsed-time target
remains unmet. Disk SST alternatives and sampled temporary peaks remain in the
raw reports rather than being described as RAM-only gains.

An intermediate numeric-only recognizer reprocessed shared-string attributes
on fallback and regressed text workloads. It was not shipped; accepting scalar
SST IDs through the same existing backend removed that regression. The final
numeric result remains faster than A7 and the equivalent calamine read modes.

These results do not include Python conversion, the unavailable NYC 311
million-row fixture, all inline/rich text or arbitrary XML layouts. They do not
resolve conservative retained-model resource accounting or constitute complete
A8 acceptance. No Pandas or Polars integration is implemented.
