"""Compare a preserved previous native binary on identical plain SST inputs."""
import argparse
import json
import os
from pathlib import Path
import statistics
import tempfile
from shared_strings_checkpoint import ROOT, HERE, generate, run

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--before", type=Path, required=True)
parser.add_argument("--before-revision", required=True)
parser.add_argument("--runs", type=int, default=5)
parser.add_argument("--rows", type=int, nargs="+", default=[10000, 100000])
parser.add_argument("--output", type=Path, default=HERE / "plain-regression.local.json")
args = parser.parse_args()
if args.runs < 1 or any(not 0 < r <= 1048576 for r in args.rows):
    parser.error("Require positive runs and valid row bounds")
target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
report = {"baseline_revision": args.before_revision, "measurement": "Same complete plain-text verification, 512MiB Memory mode; one warmup and rotating serial repetitions; runtime baseline included; builds excluded", "cases": []}
for rows in args.rows:
    for kind, unique in [("repeated", 128), ("unique", rows*10)]:
        path = HERE / "data" / f"strings-{kind}-{rows}.xlsx"
        path.parent.mkdir(exist_ok=True)
        generate(path, rows, unique)
        with tempfile.TemporaryDirectory() as temp:
            commands = {"before": [args.before, path, "memory", unique, temp], "after": [target / "release/examples/shared_text", path, "memory", unique, temp]}
            def measure(command):
                result = run([HERE / "measure", *command])
                assert result.stdout.strip() == f"{rows*10} {rows*10*110}"
                assert not list(Path(temp).iterdir())
                return json.loads(result.stderr.split("MEASURE ")[-1])
            for command in commands.values():
                measure(command)
            samples = {name: [] for name in commands}
            for iteration in range(args.runs):
                for name in (["before", "after"] if iteration % 2 == 0 else ["after", "before"]):
                    samples[name].append(measure(commands[name]))
            medians = {name: {key: statistics.median(s[key] for s in values) for key in ("seconds", "cpu_seconds", "peak_rss_kib")} for name, values in samples.items()}
            report["cases"].append({"rows": rows, "kind": kind, "samples": samples, "medians": medians, "wall_change_percent": 100*(medians["after"]["seconds"]/medians["before"]["seconds"]-1)})
            print(rows, kind, medians, flush=True)
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(report, indent=2)+"\n")
