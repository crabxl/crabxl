# Dynamic array visible-value checkpoint

Both readers stream one array formula and one numeric value per row with opaque cm/vm annotations. They verify every array expression/range and scalar. Native also checks every cache in the same pass; public cache-only validation is a separate untimed pass. No reader calculates formulas or interprets metadata graph indices. Five edge spellings and target replacement are checked by public [interop](results/m2-dynamic-formulas-interop.json).

One warmup and five rotating serial samples include process baseline, with generation/builds/readback excluded from timing. No worksheet is materialized and no temporary storage is used.

| Rows / cells | Native seconds / RSS KiB | openpyxl 3.1.5 seconds / RSS KiB |
| --- | --- | --- |
| 10,000 / 20,000 | 0.025805 / 1,932 | 0.320080 / 36,480 |
| 100,000 / 200,000 | 0.253976 / 1,932 | 1.721075 / 43,516 |

Required reference speed and desired reference RSS targets hold on this workload; these are observations, not universal guarantees. Previous core rejected these inputs, so no same-input prior performance exists. Calamine formula/cache-only overlap requires separate equivalent checks; rust_xlsxwriter has no read overlap. See [raw dynamic samples](results/m2-dynamic-formulas.json).

An existing unannotated shared/normal formula workload checks regression against exact `0faefee97d642ef057a22206a7d29ecabd4c05f0` (preserved executable SHA-256 `750d08ab87817ca2092cbc990a4ebee87706139082e7b1764fc3fba12928bf58`). At 20,000 cells, current/prior median wall is 0.029371 / 0.029382 seconds; at 200,000, 0.286849 / 0.300241 seconds. No optimization is claimed from these -0.04% / -4.5% observations. Current RSS is 1,956 KiB at both sizes, versus prior 1,968 / 1,996. All template/cache counts match, temp storage zero. [Raw regression](results/m2-dynamic-plain-formula-regression.json).

Reproduce after release example builds using `probe_dynamic_formulas.py --reader /path/to/dynamic_formula_read --editor /path/to/edit_demo`, `dynamic_formula_checkpoint.py`, and `shared_formula_checkpoint.py --baseline /path/to/preserved-example --output results.json`. Build/test activity must remain outside serial samples.

Compatible projection is explicit and counted. It does not add typed graph read/create/edit, owned metadata identities or spill dependency updates. Original editor preserves opaque parts and unaffected references; overwritten physical cells drop old annotations. M2/M4/M6 completion remains separate.
