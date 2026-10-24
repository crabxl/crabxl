# Literal structured formula flags

Public openpyxl 3.1.5 constructors/serializers verify ca, dt2D, dtr, del1 and del2 across empty, false/zero, opaque and whitespace-bearing strings. probe_literal_flags.py asserts identical formula XML and every public property after both engines save. Empty flags omit in Compatible output and become public False defaults; canonical source absence remains absent. RetainExplicit keeps empty attributes. Tests additionally cover optional Boolean meaning, compatible unused array/table hints, strict rejection, byte limits and atomic illegal-XML rejection before worksheet spooling.

Workspace tests, rustfmt, Clippy and release interoperability pass. Original reference implementation/tests are not modified or additionally inspected. Raw results: results/m2-literal-flags-interop.json, results/m2-literal-flags.json and results/m2-literal-flags-regression.json.

The typed flag workload uses public write-only creation outside timing. Both readers stream and verify every one of the five raw properties; cache projection is verified separately outside timing. One warmup plus five rotating serial cold-process samples include process/runtime baseline. Builds/tests do not overlap samples. No earlier core accepts every literal, so no equivalent prior feature benchmark is fabricated.

| Data-table cells | crabxl seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- |
| 5,000 | 0.010947 / 1,960 | 0.214200 / 34,880 |
| 50,000 | 0.104287 / 2,008 | 0.661619 / 35,900 |

Required speed and desired RSS targets hold for this overlap. No temporary storage is used for these reads; sampled peaks are zero. Neither native-competitor parity nor full structured formula editing is established.

Ordinary/shared regression uses the immediate 517245f2a77b0cca211f050dd18eea53150eaaba binary (SHA256 986eb733c7b48be73b0e1fc0b97b12bfea1f4ad16c7681fef9e668ef0df66a4c), with the same serial sampling. Both readers verify expressions; native also verifies caches in the timed pass, while public cache verification is untimed.

| Cells | Current seconds / peak RSS KiB | Prior core seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- | --- |
| 20,000 | 0.031865 / 1,976 | 0.030981 / 1,928 | 0.355232 / 36,736 |
| 200,000 | 0.303706 / 1,988 | 0.297750 / 2,012 | 2.039191 / 44,536 |

The larger current read is 2.0% slower and 24 KiB lower RSS. This is a compatibility/validation checkpoint, not an optimization claim. Shared storage remains 355 managed bytes independent of followers, and these reads use no temporary storage. M2/M4 remain open for their complete acceptance definitions.
