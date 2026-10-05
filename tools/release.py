"""Prepare and validate manually numbered alpha releases."""
import argparse
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PATTERN = re.compile(r"0\.1\.0-alpha\.([1-9][0-9]*)\Z")


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def validate(version, history=True):
    match = PATTERN.fullmatch(version)
    if not match:
        raise ValueError("Expected 0.1.0-alpha.N with a positive, unpadded integer")
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    workspace = manifest["workspace"]
    if workspace["package"]["version"] != version:
        raise ValueError("Commit the requested workspace version before releasing")
    for name in ("crabxl-core", "crabxl-xlsx"):
        if workspace["dependencies"][name]["version"] != "=" + version:
            raise ValueError(f"{name} must require exactly ={version}")
    if history:
        tags = run("git", "tag", "--list").splitlines()
        previous = [int(PATTERN.fullmatch(t)[1]) for t in tags if PATTERN.fullmatch(t)]
        number = int(match[1])
        if version in tags:
            if run("git", "rev-parse", version + "^{commit}") != run("git", "rev-parse", "HEAD"):
                raise ValueError("An existing release tag points to another commit")
            if number != max(previous):
                raise ValueError("Only the newest release can be resumed")
        elif number != max(previous, default=0) + 1:
            raise ValueError("Use the next alpha number; old checkpoints are not backfilled")
        lower = [n for n in previous if n < number]
        if lower:
            subprocess.run(["git", "merge-base", "--is-ancestor",
                            f"0.1.0-alpha.{max(lower)}", "HEAD"], cwd=ROOT, check=True)
    return version


def prepare(version):
    if not PATTERN.fullmatch(version):
        raise ValueError("Expected 0.1.0-alpha.N")
    path = ROOT / "Cargo.toml"
    text = path.read_text()
    text, count = re.subn(r'(?m)^version = "[^"]+"$', f'version = "{version}"', text)
    if count != 1:
        raise ValueError("Expected one workspace package version")
    for name in ("crabxl-core", "crabxl-xlsx"):
        text, count = re.subn(rf'({name} = \{{[^\n]*version = ")[^"]+(" \}})',
                             rf'\g<1>={version}\2', text)
        if count != 1:
            raise ValueError(f"Expected one dependency entry for {name}")
    path.write_text(text)
    subprocess.run(["cargo", "update", "--workspace"], cwd=ROOT, check=True)
    validate(version)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "check"))
    parser.add_argument("version")
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.version)
    else:
        validate(args.version)
    print(args.version)
