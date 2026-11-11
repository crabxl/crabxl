"""Decide whether this push releases, and give GitHub the version and notes.

A push to main releases the committed workspace version when no tag has that
name yet. The release workflow creates the tag after publishing. A manual run
resumes the release of the committed version.
"""
import os
import re
import subprocess
import tomllib
import uuid

from release import ROOT, run, validate


def notes(version):
    """The CHANGELOG.md section of this version, without its heading."""
    text = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    match = re.search(rf"^## \[{re.escape(version)}\][^\n]*\n(.*?)(?=^## \[|^\[[^\]]+\]: |\Z)",
                      text, re.S | re.M)
    if not match or not match[1].strip():
        raise ValueError(f"Add a CHANGELOG.md section for {version}")
    return match[1].strip()


def output(**values):
    if path := os.environ.get("GITHUB_OUTPUT"):
        with open(path, "a", encoding="utf-8") as stream:
            for key, value in values.items():
                delimiter = "value_" + uuid.uuid4().hex
                stream.write(f"{key}<<{delimiter}\n{value}\n{delimiter}\n")


def main():
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    version = manifest["workspace"]["package"]["version"]
    tagged = version in run("git", "tag", "--list").splitlines()
    if tagged and os.environ.get("GITHUB_EVENT_NAME") != "workflow_dispatch":
        output(release="false", version=version, notes="")
        print(f"{version} is already released")
        return
    validate(version)
    branch = os.environ.get("DEFAULT_BRANCH", "main")
    if subprocess.run(["git", "merge-base", "--is-ancestor", "HEAD", f"origin/{branch}"],
                      cwd=ROOT).returncode:
        raise ValueError(f"Release a commit that is on {branch}")
    output(release="true", version=version, notes=notes(version))
    print(f"Releasing {version}")


if __name__ == "__main__":
    main()
