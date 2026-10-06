# A8 descending model insertion

Compare core `d944e766ac397ea0969180c886a8affb565badc7` with
[ADR 0081](../docs/decisions/0081-packed-descending-insertion.md).
[Raw results](results/alpha8-reverse-models.json) contain identical comparison
worker source, the exact candidate patch, binary/fixture hashes and all samples.

One warmup and three rotating serial release repetitions run without overlapping
builds, tests or profiling. Creation retains a canonical editable model with a
1 GiB allowance and saves it using identical compression settings. Independent
XML readback checks every output coordinate/value; reading retains and sums both
numeric sheets. Calamine retains both returned ranges but has different editing
and source-preservation capabilities. Output checks and fixture generation are
outside timings. Temporary descriptor sampling is a lower bound, distinct from RSS.

| One million numeric cells created | Previous seconds | Candidate seconds | Previous RSS KiB | Candidate RSS KiB |
| --- | ---: | ---: | ---: | ---: |
| Descending insertion | 1.155196 | 1.146507 | 116,828 | 37,076 |
| Ascending insertion | 0.997638 | 0.990393 | 37,240 | 37,260 |

Descending peak RSS falls about 68%; its small timing change is not meaningful.
At 100,000 cells, descending creation changes from 0.104165 to 0.110248 seconds
while RSS falls from 16,512 to 8,388 KiB. Ordinary two-million-cell materialization
changes from 0.484368 to 0.494848 seconds with essentially unchanged RSS. The
unchanged calamine reference varies from 0.549333 to 0.491271 seconds across
workers, limiting interpretation of small timing differences.

Million-cell output size remains 2,942,935 bytes and sampled temporary storage
is about 35.8 MB. No streaming-write, payload-sharing or general reader gain is
claimed. The default conservative per-cell ledger remains unchanged here.
