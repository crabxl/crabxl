"""Measure automatic mode selection for scans and three repeated numeric passes."""
import json
import statistics
from run import HERE, ROOT, run


def main():
    run(["cc", "-O2", "-Wall", "-Wextra", "-Werror", HERE / "measure.c", "-o", HERE / "measure"])
    run(["cargo", "build", "--release", "--locked", "--examples"])
    cases = {"auto_scan_once": ("auto-scan", "auto", 1),
             "auto_repeat_three": ("auto-repeat", "auto", 3),
             "limited_repeat_three": ("auto-repeat", str(64 * 1024 * 1024), 3)}
    samples = {case: [] for case in cases}
    names = list(cases)
    for index in range(4):
        for case in names[index % 3:] + names[:index % 3]:
            mode, budget, passes = cases[case]
            result = run([HERE / "measure", ROOT / "target/release/examples/sum",
                          HERE / "data/numbers-1000000.xlsx", "Sheet", "32768", mode, budget, str(passes)])
            assert result.stdout.strip() == f"{10000000 * passes} {49999995000000 * passes}", result.stdout
            sample = json.loads(result.stderr.split("MEASURE ")[-1])
            sample["decision"] = next(line.removeprefix("DECISION ") for line in result.stderr.splitlines() if line.startswith("DECISION "))
            if index:
                samples[case].append(sample)
            print(case, sample, flush=True)
    report = {"workload": "Same ten-million-cell numeric input as numeric-results.json",
              "measurement": "Native Linux wait4; one warmup per case, three rotating serial measured runs; generation/build excluded; each sum verified; no temporary read storage",
              "cases": {case: {"mode": mode, "policy_budget": budget, "passes": passes}
                        for case, (mode, budget, passes) in cases.items()},
              "samples": samples,
              "medians": {case: {key: statistics.median(sample[key] for sample in values)
                                 for key in ("seconds", "cpu_seconds", "peak_rss_kib")}
                          for case, values in samples.items()}}
    (HERE / "adaptive-results.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
