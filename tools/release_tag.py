"""Validate the pushed X.Y.Z tag and give GitHub its version and changelog notes."""
import os
import re
import subprocess
import uuid

from release import ROOT, validate


def notes(version):
    """The CHANGELOG.md section of this version, without its heading."""
    text = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    match = re.search(rf"^## \[{re.escape(version)}\][^\n]*\n(.*?)(?=^## \[|^\[[^\]]+\]: |\Z)",
                      text, re.S | re.M)
    if not match or not match[1].strip():
        raise ValueError(f"Add a CHANGELOG.md section for {version}")
    return match[1].strip()


def main():
    version = os.environ["GITHUB_REF_NAME"]
    validate(version)
    branch = os.environ.get("DEFAULT_BRANCH", "main")
    if subprocess.run(["git", "merge-base", "--is-ancestor", "HEAD", f"origin/{branch}"],
                      cwd=ROOT).returncode:
        raise ValueError(f"Tag a commit that is on {branch}")
    text = notes(version)
    if output := os.environ.get("GITHUB_OUTPUT"):
        delimiter = "notes_" + uuid.uuid4().hex
        with open(output, "a", encoding="utf-8") as stream:
            stream.write(f"version={version}\n")
            stream.write(f"notes<<{delimiter}\n{text}\n{delimiter}\n")
    print(f"Validated release tag {version}")


if __name__ == "__main__":
    main()
