# A11 native source hyperlink edit checkpoint

Three fresh serial release processes per size, without overlapping builds/tests.
The generated input contains one external hyperlink and literal value per row.
Mode `edit` loads the canonical model, changes the last target, and preserves the
source package. The raw `scan` field measures this complete load/edit/save phase.

| Rows | Median edit phase | Median peak RSS | Initial generator peak temporary XML |
| --- | --- | --- | --- |
| 2,000 | 0.018702 s | 5,260 KiB | 385,622 bytes |
| 8,000 | 0.073264 s | 10,512 KiB | 1,561,622 bytes |

[Raw observations](alpha11-source-hyperlinks.txt) also include initial preparation
and initial owned save. RSS is Linux `/proc/self/status` VmHWM for the entire worker,
including input generation. Temporary bytes describe the initial writer's managed
spools; preserving save does not create worksheet spools. Caller-owned XLSX outputs
are excluded, and physical disk high-water usage was not measured.

Checksums are 2,087,884 and 32,354,884 after the last target change. Separate
openpyxl 3.1.5 readback verifies row extent, first target, last changed target,
retained last literal value and tooltip for all six outputs. There is no competitor
measurement here and no claim of a general read/write speed advantage.
