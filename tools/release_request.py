"""Validate an explicit release request and expose safe GitHub job outputs."""

import json
import os
import uuid

from release import ROOT, validate


def main():
    if os.environ.get("GITHUB_EVENT_NAME") == "workflow_dispatch":
        request = {
            "version": os.environ.get("REQUEST_VERSION", ""),
            "feature_notes": os.environ.get("REQUEST_NOTES", ""),
            "usable_feature": os.environ.get("REQUEST_USABLE") == "true",
        }
    else:
        request = json.loads((ROOT / ".github/release-request.json").read_text())
    if request.get("usable_feature") is not True:
        raise ValueError("A release must explicitly declare newly usable functionality")
    notes = request.get("feature_notes")
    if not isinstance(notes, str) or not notes.strip():
        raise ValueError("Describe the new functionality and its verified limits")
    version = validate(request["version"])
    if output := os.environ.get("GITHUB_OUTPUT"):
        delimiter = "notes_" + uuid.uuid4().hex
        with open(output, "a", encoding="utf-8") as stream:
            stream.write(f"version={version}\n")
            stream.write(f"feature_notes<<{delimiter}\n{notes}\n{delimiter}\n")
    print(f"Validated explicit release request for {version}")


if __name__ == "__main__":
    main()
