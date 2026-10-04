"""Cumulative ZIP/XML/full-read probe; diagnostic XML mode is not a XLSX reader."""
import argparse
import json
import zipfile
from run import HERE, ROOT, run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--full-binary", default=ROOT / "target/release/examples/sum")
    parser.add_argument("--output", default=HERE / "layers.local.json")
    args = parser.parse_args()
    run(["cargo", "build", "--release", "--locked", "--examples"])
    run(["cc", "-O2", "-Wall", "-Wextra", "-Werror", HERE / "measure.c", "-o", HERE / "measure"])
    path = HERE / "data/numbers-1000000.xlsx"
    with zipfile.ZipFile(path) as archive:
        size = archive.getinfo("xl/worksheets/sheet1.xml").file_size
    commands = {mode: [ROOT / "target/release/examples/read_layers", path, mode] for mode in ("zip", "xml")}
    commands["full"] = [args.full_binary, path]
    expected = {"zip": str(size), "xml": "10000000", "full": "10000000 49999995000000"}
    samples = {mode: [] for mode in commands}
    for index in range(4):
        for mode, command in commands.items():
            result = run([HERE / "measure", *command])
            assert result.stdout.strip() == expected[mode], result.stdout
            sample = json.loads(result.stderr.split("MEASURE ")[-1])
            if index: samples[mode].append(sample)
            print(mode, sample, flush=True)
    from pathlib import Path
    Path(args.output).write_text(json.dumps(samples, indent=2) + "\n")


if __name__ == "__main__":
    main()
