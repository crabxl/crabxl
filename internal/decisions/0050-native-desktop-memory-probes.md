# ADR 0050: Native desktop host-memory probes

## Decision

Auto obtains current available RAM on Windows and macOS using a RAM-only sysinfo
refresh. Version 0.38.4 is the latest stable release compatible with Rust 1.88;
0.39.6 requires Rust 1.95. The dependency is restricted to these targets and has
only its `system` feature enabled. Linux retains its existing host, mounted
cgroup hierarchy and process-limit policy without an additional dependency.

`MemorySource::NativeHost` explicitly distinguishes host observations from the
Linux effective-constraint probe. Windows job/private process restrictions and
macOS process limits are not discovered by this checkpoint. Constrained callers
must provide effective `AutoMemory.available_bytes` or an explicit budget.
Neither diagnostics nor documentation claim a process RSS ceiling.

Failed/impossible observations retain the conservative 256 MiB fallback. A valid
zero available-memory observation remains zero and may cause a budget error;
it must not be replaced with invented free memory. Explicit budgets and caller
availability continue to bypass native probing.

## Validation and scope

A deterministic test covers ordinary availability, zero under pressure and probe
failure. Windows and macOS CI exercise the real native path on Rust 1.88, alongside
existing default/backend tests. Only RAM is refreshed; no process list or CPU
sampling is requested. Native CI results are required before claiming platform
acceptance. This checkpoint does not close M2 or provide concurrency tuning.
