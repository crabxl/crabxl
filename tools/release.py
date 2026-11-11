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


def name_changelog_section(version):
    """Rename the Unreleased section of CHANGELOG.md to this version."""
    path = ROOT / "CHANGELOG.md"
    text = path.read_text(encoding="utf-8")
    if re.search(rf"^## \[{re.escape(version)}\]", text, re.M):
        return
    match = re.search(r"^## \[Unreleased\][^\n]*\n(.*?)(?=^## \[|\Z)", text, re.S | re.M)
    if not match or not match[1].strip():
        raise ValueError("Describe the changes under ## [Unreleased] in CHANGELOG.md")
    path.write_text(text[: match.start()] + f"## [{version}]\n" + text[match.start(1) :],
                    encoding="utf-8")


def replace_versions(version):
    """Install snippets and the bug report placeholder show the new version."""
    for name, pattern, replacement in (
        ("README.md", r'(?m)^crabxl = "[^"]+"$', f'crabxl = "{version}"'),
        ("docs/index.md", r'(?m)^crabxl = "[^"]+"$', f'crabxl = "{version}"'),
        (".github/ISSUE_TEMPLATE/bug_report.yml",
         r"(?m)^(\s+placeholder: )[0-9]+\.[0-9]+\.[0-9]+$", rf"\g<1>{version}"),
    ):
        path = ROOT / name
        text, count = re.subn(pattern, replacement, path.read_text(encoding="utf-8"), count=1)
        if count != 1:
            raise ValueError(f"Expected a version in {name}")
        path.write_text(text, encoding="utf-8")


def prepare(version):
    if not PATTERN.fullmatch(version):
        raise ValueError("Expected X.Y.Z without leading zeros")
    name_changelog_section(version)
    replace_versions(version)
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
