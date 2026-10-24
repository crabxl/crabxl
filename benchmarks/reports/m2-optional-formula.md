# Optional structured expression checkpoint

Canonical Formula distinguishes absent array/data-table expression from a present empty string before save. Existing expression() callers retain the XML body view; optional_expression() exposes presence without a binding-only value cache. Source empty bodies are present after read and map to the public array text "="; no cached value is fabricated.

benchmarks/probe_optional_formula.py creates native/public array inputs with ref=None and text=None, "", "=1". It verifies equivalent formula XML attributes/body, ordinary public array readback and data-only None values for all three. Results are in results/m2-optional-formula-interop.json. The public constructor still requires the ref argument; passing None explicitly is distinct from omitting it. The probe also records the pinned DataTableFormula(ref=None) ordinary-reload TypeError; canonical I/O must not introduce a crash to copy that defect. Arbitrary opaque formula reference strings and array_formulae mapping remain staged.

## Shared-read regression

One warmup and three rotating serial process wall/CPU/RSS samples on the current Linux environment, with builds excluded from timing. Every native expanded expression and cache is checked in the timed pass. openpyxl verifies expanded expressions in timing, with cache verification in a separate untimed data-only pass. These are shared-template trend measurements, not a timing comparison of literal None property access. No temporary storage occurs.

The earlier baseline is core 93951397199bde14f75d6d66da81b6fc43dcbf36, executable SHA-256 b5dfa0addaa51789d3c62fb9abca1c50b031cef9110feafea5e975bb1df9a850. It predates joint aggregate budgets and subsequent corrections, so differences do not isolate optional expression storage. Raw samples/identity are in results/m2-optional-formula-regression.json. Reproduce with shared_formula_checkpoint.py --rows 10000 100000 --runs 3 --baseline /path/to/preserved/binary --baseline-core 93951397199bde14f75d6d66da81b6fc43dcbf36 --output benchmarks/results/m2-optional-formula-regression.json.

| Verified cells | Native seconds / KiB RSS | Prior seconds / KiB RSS | openpyxl seconds / KiB RSS |
| --- | --- | --- | --- |
| 20,000 | 0.030371243 / 1,908 | 0.029468304 / 1,992 | 0.361571886 / 36,628 |
| 200,000 | 0.292516699 / 1,936 | 0.283309830 / 1,992 | 2.045721532 / 44,452 |

Native large wall time is 3.2% slower than the earlier baseline and 56 KiB lower RSS; this checkpoint makes no optimization claim. The required reference speed target holds for this measured shared-expression workload. Calamine/rust_xlsxwriter overlap comparisons remain separate acceptance work. Literal optional-expression support does not complete all formula/style/catalog or milestone requirements.
