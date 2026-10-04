# Joint managed read checkpoint

Each row has unique shared text, an expanded shared formula with integer cache, and a styled date. All readers verify text/expression/date semantics; native additionally checks caches in the same pass, while public cached values use a separate untimed pass. Native calendar serial/epoch/kind matches the reference datetime through the prior date interoperability contract. Both native workflows preload the default theme. One warmup and five rotating serial samples include process baseline, with builds/checks/generation outside timing.

Reference scan uses read_only; repeated-wide uses a full model, and repeated-tight uses read_only to match native fallback. The exact previous core is `9395139`; its policy budgets rows only, while the current policy budgets managed catalogs/cache/templates/library-retained rows together. API work includes the same observable values; previous budgets must not be presented as a global cap.

| Cells | Access / budget MiB | Current seconds / RSS KiB | Previous seconds / RSS KiB | openpyxl 3.1.5 seconds / RSS KiB |
| --- | --- | --- | --- | --- |
| 30,000 | scan / 2 | 0.049866 / 2,744 | 0.041129 / 3,448 | 0.454934 / 38,912 |
| 30,000 | scan / 256 | 0.042739 / 3,468 | 0.041526 / 3,452 | 0.450252 / 38,884 |
| 30,000 | repeated / 2 | 0.049071 / 2,736 | 0.042241 / 3,448 | 0.455522 / 38,884 |
| 30,000 | repeated / 256 | 0.050063 / 11,920 | 0.047312 / 11,892 | 0.440189 / 50,144 |
| 300,000 | scan / 2 | 1.058692 / 3,504 | 0.400818 / 16,892 | 3.148728 / 65,652 |
| 300,000 | scan / 256 | 0.417996 / 16,960 | 0.418815 / 16,884 | 3.172327 / 66,048 |
| 300,000 | repeated / 2 | 1.060331 / 3,468 | 0.424624 / 16,888 | 3.209771 / 65,788 |
| 300,000 | repeated / 256 | 0.483214 / 102,612 | 0.471056 / 102,596 | 4.269434 / 185,768 |

At 300,000 cells, narrow scan takes 1.058692 seconds versus wide 0.417996: memory is used to avoid repeated disk lookups when the allowance permits. Narrow policy peak RSS is 3,504 KiB versus prior 16,892; its managed assertion fits the 2MiB budget minus configured working reserve. Process RSS still exceeds the managed budget because runtime/allocator/dependency storage is additional. The narrow path uses exactly 12,400,000 anonymous data/index bytes (1,240,000 at 30,000 cells), while wide/prior paths use zero. Directory sampling reports zero because files are unlinked; exact native stats, not directory sampling, establish this tradeoff. OS page cache is additional; owned files close on workbook Drop, and failed-spill cleanup/source integrity has deterministic tests.

Required reference speed and desired lower reference RSS hold for all eight observed workloads. The joint narrow policy is slower than the former rows-only policy: +164% wall at the large scan, in exchange for honoring the aggregate allowance and lower RSS. Wide scan wall is approximately unchanged; wide materialization is +2.6% at the large case. No universal optimization or calamine/rust_xlsxwriter overlap win is claimed.

An unconfigured direct shared/normal formula workload separately checks regression. At 20,000 cells, current/prior median wall is 0.029603 / 0.030679 seconds; at 200,000 it is 0.295255 / 0.281501 (+4.9%). Current RSS is 1,944 / 1,956 KiB, prior 1,992 / 1,996. The large-case time regression remains visible; required reference speed still holds against 0.362687 / 2.068213 seconds. Temporary storage is zero. These are observations, not stable timing thresholds.

Raw [mixed policies](results/m2-aggregate-read.json) include inspected managed bytes, source modes, exact temporary sizes and all samples; inspection is the per-row maximum for streams or final retained model ledger, not a replacement for OS peak RSS. [Direct regression](results/m2-aggregate-direct-formula-regression.json) includes all runs. Reproduce with release adaptive_mixed_read/shared_formula_read examples, `aggregate_read_checkpoint.py --baseline /path/to/prior-mixed --baseline-core 9395139`, and `shared_formula_checkpoint.py --baseline /path/to/prior-shared --output results.json`.

Preserved prior executables SHA-256: mixed `1fe7422933e167105e24e69b10f4813f28aa83cda1564d20fe0ddb87a098a5c6`; direct shared `b5dfa0addaa51789d3c62fb9abca1c50b031cef9110feafea5e975bb1df9a850`. They use the exact previous engine with the common mixed example; prior-only inspection stubs are inactive in timed runs because that engine lacks the new ledger API. No engine code is modified in the baseline.

This checkpoint does not complete owned bank/catalog mutation, broad portable memory probing or all baseline milestones. Managed operation accounting and a hard process RSS limit remain distinct contracts.
