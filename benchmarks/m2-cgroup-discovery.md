# Mounted-controller Auto policy checkpoint

Reproduce after building and preserving the fa5f3b0 baseline sum example, then building the current locked release example:

```sh
python benchmarks/cgroup_checkpoint.py --baseline /workspace/scratch/sum-before-cgroup --current /workspace/openrsxl/target/release/examples/sum
```

[Raw results](results/m2-cgroup-discovery.json) retain every sample, source hash, expected count/checksum, native Auto decision and preserved baseline binary hash. One warmup and five rotating serial samples include native/Python process startup; fixture creation, builds and tests are excluded. Both scans stream once. Repeated-access native Auto samples and materializes when the allowance fits; the public reference uses ordinary loaded cells and three iterations. Models/validation differ, so this is supported numeric overlap, not universal parity.

| Cells | Workload | Current seconds / RSS KiB | Prior seconds / RSS KiB | openpyxl seconds / RSS KiB |
|---|---|---|---|---|
| 50,000 | Scan once | 0.04101 / 2,060 | 0.03853 / 1,932 | 0.38096 / 35,636 |
| 50,000 | Sum three times | 0.04135 / 4,680 | 0.04257 / 4,684 | 0.48520 / 55,784 |
| 500,000 | Scan once | 0.37548 / 2,060 | 0.37675 / 1,996 | 2.23182 / 39,452 |
| 500,000 | Sum three times | 0.40807 / 29,272 | 0.40284 / 29,260 | 4.81484 / 240,004 |

Native current/prior wall differences range from -2.9% to +6.4%; RSS differences range from -4 to +128 KiB. The larger scan changes -0.3% and larger repeated case +1.3%. This fixes controller discovery coverage, not a claimed throughput optimization. Required speed and desired RSS relative to openpyxl hold here; native competitor targets remain independently open.

The live cgroup v2 namespace reports mount root /.. and process membership /. Visible mount-root headroom is inspected without filesystem traversal. Every current sample reports Linux discovery with about 0.94 GiB observed effective availability and roughly 177 MiB derived allowance, varying with live usage. Scans remain Streaming and repeated access is Materialized. Hidden constraints beyond the visible hierarchy, non-Linux probes and concurrent strategies remain staged; this is a snapshot rather than a hard RSS ceiling. Actual cgroup v1-host performance is not claimed: synthetic filesystem tests verify v1/hybrid, ancestor limits, mount mapping and missing/malformed data.

Temporary bytes sampled every 25ms are zero, with cleanup checked after each process; sampling can miss short-lived files. Source ZIP is outside the sampled directory. No new read spool was introduced. Workspace correctness tests, rustfmt and Clippy pass. This checkpoint does not complete M2.
