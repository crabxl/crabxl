# Printing component update checkpoint

[Raw results](results/m5-printing-updates.json) retain five rotating serial samples after one warmup, preserved baseline binary SHA256, source hashes, managed overlay bytes, output sizes and Linux wall/CPU/RSS. Builds/tests/fixture generation and public metadata/every-cell verification do not overlap timing. Repeated updates change only the left margin twenty times, followed by one save; unrelated large page-break lists remain intact. This stresses metadata ownership rather than claiming every printing operation accelerates equally.

Build the identical snapshot worker against the published prior core by adding a detached worktree at 8136b1d837e8f5ef6e67f32d2a1effbe7c9d2341 and copying crates/crabxl/examples/printing_snapshot_updates.rs plus examples/support/printing_updates.rs into its matching directories. Build that worker with --release --locked, preserve its binary outside the target directory, then rebuild the current snapshot/component examples. If sharing Cargo targets between checkouts, invalidate root core build inputs before rebuilding to prevent stale artifacts.

```sh
python benchmarks/printing_updates_checkpoint.py --baseline /workspace/scratch/printing-snapshot-before-updates --target /workspace/openrsxl/target/release/examples
```

| Rows / breaks | Prior snapshot seconds / RSS KiB | Current snapshot seconds / RSS KiB | Current component seconds / RSS KiB | openpyxl seconds / RSS KiB |
|---|---|---|---|---|
| 5,000 / 1,000 | 0.14251 / 2,828 | 0.06276 / 2,884 | 0.05560 / 2,748 | 0.24953 / 39,212 |
| 50,000 / 10,000 | 1.37951 / 4,408 | 0.58457 / 3,980 | 0.55331 / 3,236 | 0.91521 / 77,768 |

Current full snapshot replacement benefits from reusable source validation; the component API also avoids cloning unrelated vectors and retaining a second caller snapshot. For the larger case, component wall time falls about 60% and peak RSS about 27% versus the prior snapshot workflow. Versus current snapshot, component wall time falls about 5% and RSS about 19%. Small current-snapshot RSS rises 56 KiB versus prior; results do not claim universally lower RSS. Required speed and desired RSS versus openpyxl hold for this supported workload, with different native/Python process baselines and model ownership disclosed. No native-competitor result is inferred.

All native managed overlays charge 57,344 bytes for the smaller list and 561,344 for the larger list in these fixtures. Counters are not RSS: caller snapshot ownership, temporary replacement clones, parser buffers, allocator/dependency costs and runtime baseline are additional. Component updates preserve actual source vector capacities; another source could retain more slack than a cloned length-sized vector. Tests independently verify pointer identity and zero source reads after first validation.

Native editor output uses no worksheet spool; temporary sampling is zero with cleanup checked. Reference sampled peaks are 201,087 and 3,256,098 bytes. Sampling every 25ms can miss short-lived files; final ZIP/source are outside the sampled directory. Public descriptor signatures include all print fields and every break record; all numeric cells are checked outside timing. Repeated save, atomic memory/XML/relationship failures, source clear/reload and aggregate workbook limits are separately tested. This does not close M5 or implement Python printing proxies, printer graph changes, headers/footers or print names.
