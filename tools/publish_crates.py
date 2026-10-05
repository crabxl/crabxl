"""Publish dependency-ordered crates, safely resuming identical uploaded packages."""
import hashlib
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request

from release import ROOT, validate


def published(name, version):
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}/{version}",
        headers={"User-Agent": "crabxl-release (https://github.com/crabxl/crabxl)"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)["version"]
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise


def main(version):
    validate(version)
    if not os.environ.get("CARGO_REGISTRY_TOKEN"):
        raise ValueError("Configure the CARGO_API_TOKEN GitHub Actions secret")
    for name in ("crabxl-core", "crabxl-xlsx", "crabxl"):
        # Registry propagation may lag behind the preceding dependency upload.
        for attempt in range(12):
            result = subprocess.run(["cargo", "package", "-p", name, "--locked"], cwd=ROOT)
            if result.returncode == 0:
                break
            if attempt == 11:
                result.check_returncode()
            time.sleep(10)
        archive = ROOT / "target" / "package" / f"{name}-{version}.crate"
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
        existing = published(name, version)
        if existing:
            if existing["checksum"] != checksum or existing["yanked"]:
                raise ValueError(f"{name} {version} exists with different content or is yanked")
            print(f"Already published identical {name} {version}", flush=True)
        else:
            subprocess.run(["cargo", "publish", "-p", name, "--locked"], cwd=ROOT, check=True)
        for attempt in range(12):
            existing = published(name, version)
            if existing and existing["checksum"] == checksum and not existing["yanked"]:
                break
            if attempt == 11:
                raise RuntimeError(f"Could not verify published {name} {version}")
            time.sleep(10)


if __name__ == "__main__":
    main(sys.argv[1])
