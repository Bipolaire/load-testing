#!/usr/bin/env python3
"""Émet une configuration cargo-packager avec la version courante.

Usage : python3 scripts/packager-config.py <macos-arm64|macos-x64|windows-x64>

La version est lue dans `LT_VERSION`, sinon dans `GITHUB_REF_NAME` (tag `v1.2.3`).
Le JSON est écrit sur la sortie standard : cargo-packager accepte une chaîne JSON
en plus d'un chemin de fichier.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    config_path = ROOT / "packager" / f"{sys.argv[1]}.json"
    config = json.loads(config_path.read_text(encoding="utf-8"))

    version = os.environ.get("LT_VERSION", "").strip()
    if not version:
        version = os.environ.get("GITHUB_REF_NAME", "").strip().lstrip("v")
    if version:
        config["version"] = version

    print(json.dumps(config))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
