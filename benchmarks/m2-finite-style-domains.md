# Finite style domain compatibility checkpoint

Public openpyxl 3.1.5 constructors and save/reload accept font sizes -1, 410 and 1,000,000, gradient path edges -1 and 2, and an empty number-format string. The canonical reader and source-catalog writer now reproduce those finite properties. Color selection follows indexed/theme/auto/RGB priority; synthetic reader tests also cover invalid ignored identities, invalid XML entities and unknown attributes. Non-finite style data remains rejected.

Run `python benchmarks/probe_finite_style_domains.py` with CARGO_TARGET_DIR pointing to release examples. The generated reference file is first verified through public reload. Its zero-record tableStyles/dxfs declarations are removed explicitly because typed table/differential style export remains staged; no nonempty declaration or original-package preservation is tested. Canonical read/new-package export and public readback then verify the listed properties. Existing style-domain evidence and its original probe remain unchanged. See results/m2-finite-style-domains-interop.json and ADR 0029.

## Creation regression

One warmup and three rotating serial process samples on the current Linux environment, without builds during timing. Every cell's public properties and native values are verified outside timing. The prior executable is core 7b3ee6120d1beeec4eda1a7562bc2b9035c72a54, SHA-256 4b09109c685ca7b952c1cae93484c02ef6297a7b2788397f4e81e0223acf52b2. It predates source-date export as well as this checkpoint, so differences do not isolate validation changes.

| Combinations/cells | Native seconds / KiB RSS | Prior seconds / KiB RSS | openpyxl seconds / KiB RSS |
| --- | --- | --- | --- |
| 1,000 | 0.034817145 / 2,520 | 0.034230298 / 2,624 | 0.234375772 / 39,124 |
| 8,000 | 0.261825734 / 3,976 | 0.255436025 / 3,920 | 0.611976841 / 71,128 |

The large native trend is 2.5% slower and 56 KiB higher RSS than the earlier baseline. This is a correctness checkpoint, not an optimization claim. Exact native worksheet spool peaks remain 55,748 and 475,748 bytes. Public temporary storage is sampled every 25 ms and can miss short peaks; medians are 53,412 and 524,005 bytes. Native output retains four extra date presets and emits larger styles XML; equivalent visible properties do not imply identical serialization work. No native competitor creation claim is made. Raw samples are in results/m2-finite-style-domains-regression.json.

## Read/catalog regression

Five rotating serial samples plus warmup verify every one of 100,000 mixed values with 1,001 or 50,001 declared number formats. No worksheet materialization or temporary storage occurs. The preserved prior reader is 541d8dfe83704ded24ceb3a81aa982ea254fb6c7, SHA-256 84eb509ef4deaf318abdafca516229482fcb21b274b8ac526d8699cd4bf79dea. It predates cached canonical classification and subsequent checkpoints, so this trend cannot isolate the domain change.

| Declared formats | Native seconds / KiB RSS | Prior seconds / KiB RSS | openpyxl seconds / KiB RSS |
| --- | --- | --- | --- |
| 1,001 | 0.091549874 / 2,056 | 0.090977805 / 2,044 | 0.542340649 / 34,804 |
| 50,001 | 0.124107357 / 4,700 | 0.123823928 / 5,064 | 0.837370207 / 89,128 |

Large native wall time is 0.2% slower; RSS is 364 KiB lower than that earlier classification baseline. Previously removed classification scratch explains the memory trend; no new domain-validation memory optimization is asserted. Exact and sampled temporary storage are zero. Raw samples are results/m2-finite-style-domains-reader.json. These tests do not cover full Python style interfaces or claim calamine/rust_xlsxwriter comparison acceptance.
