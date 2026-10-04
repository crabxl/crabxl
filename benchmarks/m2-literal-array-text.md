# Literal array text and source body checkpoint

Formula::from_array_text retains the optional public property in a single payload. array_text() borrows owned literal spelling and produces source spelling only when requested. The XML body view skips the first Unicode character, including non-equals prefixes. Source construction clears the literal origin marker. Ordinary Rust formula construction keeps its previous optional-equals behavior; this compatibility model does not impose array slicing on ordinary formulas.

probe_literal_array_text.py verifies None, empty, "=", "=1", "1", "abc", "==1" and a non-ASCII first character against public openpyxl 3.1.5 assignment/save/reload. All eight formula XML records and all ordinary/data-only values agree. Older optional-formula fixture/probe remains reproducible and unchanged. Raw results are results/m2-literal-array-text-interop.json. This does not broaden reference-string geometry or complete array_formulae mapping.

## Shared-template regression

One warmup plus three rotating serial process wall/CPU/RSS samples; every native expanded expression/cache is checked, with public cache verification in a separate untimed pass. No temporary storage, worksheet materialization or concurrent builds occur. This is a shared-reader regression trend, not a timing comparison of literal array getters. The prior binary is core 93951397199bde14f75d6d66da81b6fc43dcbf36, SHA-256 b5dfa0addaa51789d3c62fb9abca1c50b031cef9110feafea5e975bb1df9a850; it predates aggregate budgets and later corrections and does not isolate this change. Raw samples/identity are results/m2-literal-array-text-regression.json.

| Verified cells | Native seconds / KiB RSS | Prior seconds / KiB RSS | openpyxl seconds / KiB RSS |
| --- | --- | --- | --- |
| 20,000 | 0.031997854 / 1,948 | 0.030036546 / 2,072 | 0.348828046 / 36,736 |
| 200,000 | 0.294450169 / 1,988 | 0.282303255 / 1,992 | 2.102967745 / 44,540 |

Large native wall time is 4.3% slower than that earlier baseline; RSS is similar. This is a semantic correction, with no optimization claim. Reference speed targets hold for the measured shared-read workload. Native competitor overlap and Python conversion measurements remain separate requirements.
