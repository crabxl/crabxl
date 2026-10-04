# Derived calculation-chain checkpoint

The generated input has one formula in column B per row with a valid original numeric cache, nine numeric cells per row and a conventional nonstandard-path calculation chain. The timed operation loads, replaces A1 with -1 and saves. Native Rust and openpyxl general mode have equivalent visible numeric/formula output on full public readback. This is a native comparison; shared Python parity tests exercise the binding separately.

One warmup and three alternating release runs at each scale; Rust 1.88, Python 3.12, openpyxl 3.1.5. Kernel wait4 records wall/CPU/RSS. Generation and validation are outside timing. Temporary files are polled at 10 ms, excluding named input/final output XLSX files but including an adjacent Rust output temporary. No worksheet/chain DOM is retained in Rust.

| Rows | Engine | Wall seconds | CPU seconds | RSS MiB | Extra observed temp MiB |
|---|---|---:|---:|---:|---:|
| 10,000 | openrsxl-native | 0.670 | 0.669 | 2.62 | 0.41 |
| 10,000 | openpyxl | 1.160 | 1.351 | 82.72 | 3.54 |
| 100,000 | openrsxl-native | 6.726 | 6.725 | 2.64 | 3.92 |
| 100,000 | openpyxl | 12.669 | 12.866 | 532.89 | 37.35 |

Every output retains all formulas, contains no nonempty stale formula cache, passes numeric count/checksum, and omits the derived chain and both package declarations. Full openpyxl read-only output confirms all visible values and formula coordinates. Temporary cleanup passes. Rust meets the faster-than-openpyxl requirement and desired lower RSS on these samples. Default removal intentionally does not parse old chain records: they are invalidated derived data, not retained feature access. Unchanged saves preserve them; unsafe graphs reject edits.

| Numeric no-chain rows | Before wall seconds | Current wall seconds | Change |
|---|---:|---:|---:|
| 10,000 | 0.6851 | 0.6994 | +2.1% |
| 100,000 | 6.8034 | 6.6792 | -1.8% |

The added no-chain workbook relationship scan has no consistent measured speed improvement or regression at these two scales. These results do not resolve earlier reader or Python-bank regression items. Unknown-feature structural editing, default-content-type chain discovery and more advanced graph transformations remain staged.

```sh
python benchmarks/chain_checkpoint.py --before-editor /path/to/pre-chain/edit_demo
```

[Raw evidence](results/m4-chain.json) includes CPU, RSS, temporary storage, output sizes, checksums and chain XML size for all runs.
