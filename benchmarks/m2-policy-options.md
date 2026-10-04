# Composable policy and projected shared-formula checkpoint

One warmup and five rotating serial cold-process samples; no concurrent builds or checks. Both sizes request the first 1,000 formula rows. Native readers validate expressions and integer caches, consume all XML and validate CRC. The public openpyxl 3.1.5 projected iterator stops before the tail; its cached values are checked separately outside timing. Requested outputs agree, but equal parser validation work is not claimed.

| Source rows | Current seconds / RSS KiB | Previous seconds / RSS KiB | openpyxl seconds / RSS KiB |
| --- | --- | --- | --- |
| 10,000 | 0.015113 / 2,120 | 0.020777 / 3,504 | 0.238163 / 36,428 |
| 100,000 | 0.132108 / 2,120 | 0.207052 / 14,740 | 0.574461 / 44,492 |

Current retains 1,000 shared masters in both cases; previous retains all 10,000/100,000. Neither native reader retains worksheet XML/cells or creates temporary storage. Current uses a 2MiB joint managed budget. Previous uses direct ReadOptions because its adaptive API did not compose those options; identical resource policies are not claimed. The required reference speed and desired reference RSS targets hold for these workloads. No calamine or rust_xlsxwriter overlap claim is made.

Previous revision: 54ca66273f2073063ec1ffca40b69c9f5c2a2490. Preserved binary SHA-256: c98bece666ab4e0f6a03c962cc0a1826721a15f5185d01a931525693e7279126. The common driver substitutes the previous direct-options API without changing that engine. Raw samples and semantics: results/m2-policy-options.json; reproduction: policy_options_checkpoint.py and crates/crabxl/examples/policy_projected_formulas.rs.

Managed allocation bounds are distinct from process RSS. Strict full-group validation intentionally retains later groups and remains subject to configured template limits. No milestone completion is claimed.
