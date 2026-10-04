"""Compare first-pass streaming and explicitly materialized numeric reads."""
import json
import statistics

from run import HERE, ROOT, measure, run


def main():
    run(["cc", "-O2", "-Wall", "-Wextra", "-Werror", HERE / "measure.c", "-o", HERE / "measure"])
    run(["cargo", "build", "--release", "--locked", "--examples"])
    path = HERE / "data/numbers-1000000.xlsx"
    commands = {mode: [ROOT / "target/release/examples/sum", path, "Sheet", 32768, mode, 1024 ** 3]
                for mode in ("stream", "materialized")}
    samples = {mode: [] for mode in commands}
    for command in commands.values():
        measure(command, 10000000)
    for index in range(3):
        for mode in (list(commands) if index % 2 == 0 else list(reversed(commands))):
            sample = measure(commands[mode], 10000000)
            samples[mode].append(sample)
            print(f"{mode} {sample}", flush=True)
    result = {
        "rows": 1000000, "columns": 10, "cells": 10000000,
        "checksum": 49999995000000,
        "input_buffer_bytes": 32768, "materialized_data_budget_bytes": 1024 ** 3,
        "measurement": "Same environment/input as numeric-results.json; native Linux wait4; one warmup per mode, alternating serial order, three measured runs; load plus one sum, no baseline subtraction",
        "samples": samples,
        "medians": {mode: {key: statistics.median(sample[key] for sample in values)
                           for key in ("seconds", "cpu_seconds", "peak_rss_kib")}
                    for mode, values in samples.items()},
    }
    (HERE / "mode-results.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
