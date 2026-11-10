"""Prepare and validate manually numbered releases (X.Y.Z)."""
import argparse
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PATTERN = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\Z")


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def number(version):
    return tuple(int(part) for part in PATTERN.fullmatch(version).groups())


def check_history(version):
    """A new version is newer than every release tag and builds on the newest."""
    tags = [tag for tag in run("git", "tag", "--list").splitlines() if PATTERN.fullmatch(tag)]
    if version in tags:
        if run("git", "rev-parse", version + "^{commit}") != run("git", "rev-parse", "HEAD"):
            raise ValueError("An existing release tag points to another commit")
        if number(version) != max(map(number, tags)):
            raise ValueError("Only the newest release can be resumed")
    elif tags and number(version) <= max(map(number, tags)):
        raise ValueError("Use a version newer than every release tag")
    lower = [tag for tag in tags if number(tag) < number(version)]
    if lower:
        subprocess.run(["git", "merge-base", "--is-ancestor", max(lower, key=number), "HEAD"],
                       cwd=ROOT, check=True)


def validate(version, history=True):
    if not PATTERN.fullmatch(version):
        raise ValueError("Expected X.Y.Z without leading zeros")
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    workspace = manifest["workspace"]
    if workspace["package"]["version"] != version:
        raise ValueError("Commit the requested workspace version before releasing")
    for name in ("crabxl-core", "crabxl-xlsx"):
        if workspace["dependencies"][name]["version"] != "=" + version:
            raise ValueError(f"{name} must require exactly ={version}")
    if history:
        check_history(version)
    return version


def prepare(version):
    if not PATTERN.fullmatch(version):
        raise ValueError("Expected X.Y.Z without leading zeros")
    path = ROOT / "Cargo.toml"
    text = path.read_text()
    text, count = re.subn(r'(?m)^version = "[^"]+"$', f'version = "{version}"', text)
    if count != 1:
        raise ValueError("Expected one workspace package version")
    for name in ("crabxl-core", "crabxl-xlsx"):
        text, count = re.subn(rf'(?m)^({name} = \{{[^\n]*?\bversion = ")[^"]+(")',
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
