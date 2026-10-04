# Differential and table/pivot style definition checkpoint

Canonical catalogs/codecs now support sparse differential font/number/fill/alignment/border/protection overrides and named table/pivot definitions, defaults, all 28 region tokens and optional count/size/reference properties. Actual vector capacities and payloads are budgeted; advertised counts never allocate storage. Source-style ownership transfer retains font payload pointers; invalid references and extensions fail new-package construction explicitly. Original unchanged stylesheet preservation remains a distinct editor capability.

## Public definition interoperability and performance

style_extras_checkpoint.py generates metadata with public openpyxl 3.1.5 constructors/serializers and injects it into a generated workbook containing one scalar cell. Definitions are unused by worksheet instances. Both engines read and export the definitions and scalar; all selected differential properties, 28 regions/references, defaults and optional count are checked through public metadata classes and workbook scalar access outside timing. The native path additionally verifies every differential font identity. It transfers the source theme's immutable ownership and creates a new package; this is not preservation of conditional-formatting/table instances or advanced graphs.

Five rotating serial process wall/CPU/RSS samples plus warmup on the current Linux environment exclude builds, generation and public readback. No equivalent previous native differential export API exists, so no prior-equivalent comparison is fabricated. No native competitor feature overlap claim is made. Raw samples are results/m2-style-extras.json.

| Definitions | Native seconds / KiB RSS | openpyxl seconds / KiB RSS | Native managed style bytes |
| --- | --- | --- | --- |
| 1,000 | 0.047342895 / 3,764 | 0.326496323 / 43,560 | 731,582 |
| 10,000 | 0.434957826 / 11,144 | 1.679918872 / 121,940 | 7,663,622 |

Reference speed/RSS targets hold for this workload. Native styles XML is 371,144 / 3,674,145 bytes versus reference 370,329 / 3,673,330 bytes, including native's extra date presets. Equivalent definition properties do not imply identical internal serialization work. Native exact worksheet spool peak is 206 bytes at both sizes; sampled native medians are also 206. Public sampling medians are zero despite worksheet spooling; 25ms polling can miss short peaks and is not an exact disk total. All native-owned temporary files are checked cleaned up. Final ZIP output is excluded from spool accounting. Managed style bytes include capacities/boxes/indices and exclude theme/input/runtime/allocator overhead; they are not a hard RSS cap.

## Default creation regression

Three rotating serial samples plus warmup verify all ordinary shared font/fill/alignment/value properties outside timing. The preserved earlier baseline is 7b3ee6120d1beeec4eda1a7562bc2b9035c72a54, SHA-256 4b09109c685ca7b952c1cae93484c02ef6297a7b2788397f4e81e0223acf52b2. It predates source export and subsequent corrections, so this trend does not isolate added definition models.

| Combinations/cells | Native seconds / KiB RSS | Prior seconds / KiB RSS | openpyxl seconds / KiB RSS |
| --- | --- | --- | --- |
| 1,000 | 0.034466617 / 2,616 | 0.033935081 / 2,624 | 0.235240347 / 39,120 |
| 8,000 | 0.257270064 / 4,012 | 0.255413259 / 3,980 | 0.576044801 / 71,120 |

Large native wall time is 0.7% slower with 32 KiB higher RSS than that earlier baseline. Empty additional model fields add 32 accounted registry bytes on this target; no default-path optimization claim is made. Exact native spool peaks remain 55,748 / 475,748 bytes. Public sampled medians are 28,755 / 501,047 bytes and can miss peaks. Raw samples are results/m2-style-extras-default-regression.json. Full bank editing, Python style objects, source extension payloads and worksheet rule/table graphs remain required.
