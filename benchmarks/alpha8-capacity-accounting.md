# A8 capacity-aware model allowances

Compare core `7160f140fa6ed9ed66d0f47d087b6e3ea3c1d061` with
[ADR 0082](../docs/decisions/0082-capacity-aware-model-accounting.md).
[Numeric operations](results/alpha8-capacity-numeric.json),
[text/style models](results/alpha8-capacity-text-styles.json) and the
[public resource probe](results/alpha8-capacity-probe.json) retain source patches,
worker/probe source, hashes, individual samples and verification boundaries.

The functional probe populates 100,000 rows by ten columns, verifies every integer
and coordinate, and checks the complete checksum. It is not a timed sample or the
unavailable NYC workbook. With the same 64 MiB managed allowance, the previous
ledger rejects after 262,143 cells; the candidate completes one million cells,
charging 36,000,261 bytes. Ascending and descending populations agree. Under
384 MiB, both versions populate one million cells, but only the candidate can
insert a row and verify all shifted coordinates within its structural allowance.

These are better managed reservations, not a measured physical RSS reduction.
Each block reserves a conservative tree allowance and charges actual vector
capacity; payload accounting and explicit count/byte controls remain enforced.
Buffer overlap, guarded shrinking, stable aggregates, failure atomicity and
SST RAM/disk cleanup are checked within existing resource workflows.

One warmup and three rotating release samples run serially, with no overlapping
builds, tests, profiling or generation. Numeric creation retains and saves one
sheet; reads/edits retain both numeric sheets. Every output cell is independently
verified outside timing. Text/style workers verify all retained values inside
timing and forced disk cleanup after each process. Calamine retains noneditable
ranges; editing/source-preservation capabilities are not equated.

| Large complete operation | Previous seconds | Candidate seconds |
| --- | ---: | ---: |
| Two million numeric cells, materialize | 0.483468 | 0.482310 |
| Two million numeric cells, load/edit/save | 2.977165 | 3.031241 |
| One million numeric cells, per-cell ascending creation/save | 0.926147 | 0.962872 |
| One million numeric cells, per-cell descending creation/save | 1.070170 | 1.118683 |
| One million repeated text cells, RAM model | 0.391256 | 0.367977 |
| One million unique text cells, RAM model | 0.755693 | 0.720358 |
| One million unique text cells, disk model | 1.282021 | 1.281650 |
| One million styled cells, model | 0.516646 | 0.513102 |

Large ordinary reading is preserved; per-cell creation costs about 4-5% more.
Small timings are mixed, including slower descending creation. RSS is essentially
unchanged: 69,172 KiB numeric read, 225,204 KiB unique RAM text and 209,524 KiB disk text.
Numeric output archives remain 2,942,935 bytes; unique disk temporary sampling
remains 126,000,000 bytes with cleanup verified. No writer superiority is claimed.

An initial implementation and cache refinement showed read/style regressions.
Inlining the two checked plan helpers avoids their per-cell out-of-line result
transfer; the intermediate symbols measured 270 and 799 bytes. The raw reports
retain intermediate large numeric medians and a separate 299-sample profile,
dominated by deflate, so those regressions are visible. Helper inlining has a
code-size cost and does not imply all formats improve. Accept this change for
budget usability with preserved large read cost; keep creation overhead and
conservative structural/per-alias charging as explicit tuning opportunities.

Python integration and final release/platform acceptance remain separate gates.
