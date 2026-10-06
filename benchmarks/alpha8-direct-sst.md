# A8 simple shared-string preparation

Compare the pushed scalar checkpoint `bfeb2714dcea31f0facaa5c999547b071e7a55c8`
with [ADR 0078](../docs/decisions/0078-buffered-plain-shared-text.md). Exact source
patches, binary and fixture hashes, samples, resource settings and temporary
storage observations are in the [text/style report](results/alpha8-direct-sst-models-validation.json)
and [numeric report](results/alpha8-direct-sst-numeric-validation.json).

All native release processes ran serially without overlapping compilation,
tests, profiling or generation. Text/style workloads use a warmup and three
rotating samples, checking every retained value and coordinate. Numeric
regression validation uses a warmup and five rotating samples. Calamine 0.36.1
retains noneditable ranges; preserve/edit capabilities are not equated.

| Workload | Scalar checkpoint seconds | Candidate seconds | Calamine seconds |
| --- | ---: | ---: | ---: |
| One million unique shared-text cells, RAM SST model | 0.950237 | 0.733566 | 0.738130 |
| One million unique shared-text cells, disk SST model | 1.476138 | 1.292198 | not compared |
| One million repeated shared-text cells, RAM SST model | 0.384709 | 0.374593 | 0.430564 |
| One million styled numeric cells, model | 0.532764 | 0.551883 | not compared |
| Two million numeric cells, model | 0.477545 | 0.473091 | 0.513902 |
| Two million numeric cells, stream | 0.345418 | 0.353656 | 0.400963 |

Unique RAM SST model RSS is 225,052 KiB versus calamine's 309,128 KiB. The disk
model uses 209,436 KiB and a sampled 126,000,000-byte temporary peak; cleanup is
verified after each process. Disk remains slower than RAM. Model allowances
remain conservative and distinct from observed RSS.

The unique-text gain is approximately 23% against the preceding scalar
checkpoint. A preliminary non-inlined trial showed a roughly 5% numeric model
regression; the five-sample validation does not reproduce it. Stream and styled
changes remain mixed, so no across-the-board improvement is claimed. The small
elapsed difference from calamine does not establish stable superiority.

Python conversion, arbitrary Unicode/attribute/rich XML layouts, the unavailable
NYC fixture, write/edit throughput and retained-model accounting remain separate
A8 gates. No dataframe integration or complete performance acceptance is claimed.
