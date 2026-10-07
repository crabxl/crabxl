# A11 native streamed hyperlink metadata

[Release example](../crates/crabxl/examples/stream_hyperlinks.rs), three fresh serial
processes per size without test/build overlap. One reused Row contains a literal
`value`; each row has a distinct external target. No worksheet model or growing
hyperlink collection is retained. Metadata allowance is 128 KiB with fixed buffers.

| Rows | Median append | Median close | Median ZIP finish | Median process peak RSS | Peak logical temporary bytes |
| --- | --- | --- | --- | --- | --- |
| 8,000 | 0.009592 s | 0.000173 s | 0.012395 s | 2,140 KiB | 2,887,549 |
| 64,000 | 0.069961 s | 0.001338 s | 0.085354 s | 2,196 KiB | 23,591,556 |

[Raw observations](alpha11-stream-hyperlinks.txt) also record retained temporary
bytes before ZIP packaging: 2,577,763 and 20,989,768. The close peak includes the
brief overlap between the declaration spool and its copied worksheet records.
This is managed logical storage, including buffered bytes; physical filesystem
high-water usage was not sampled. Caller-owned output archives are excluded.

Linux VmHWM measures the entire native worker. The eightfold row increase grows
disk usage while RSS stays close to the runtime/buffer baseline in these samples.
Checksums are 334,890 and 2,740,890. Separate openpyxl 3.1.5 normal readback verifies
every literal value, target, generated identity and row/column extent in all six
outputs, outside native RSS/time probes. These inputs differ from the owned-model
hyperlink probe; no cross-mode or competitor speed advantage is claimed.
