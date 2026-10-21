"""Local regression checks over real UI captures; optional Python/Pillow tooling.

Run after audit-navigation.ps1. These workload-specific limits are not CI gates
or replacements for the product's startup/working-set targets.
"""
import argparse
import json
from pathlib import Path

from PIL import Image


def check(root: Path) -> None:
    samples = json.loads((root / "navigation.json").read_text(encoding="utf-8-sig"))
    warm = next(row for row in samples if row["stage"] == "cycle-2")
    growth = samples[-1]["privateMiB"] - warm["privateMiB"]
    failures = []
    print(f"Private memory growth after cycle 2: {growth:.2f} MiB")
    if growth > 25:
        failures.append("navigation private-memory growth exceeds the local 25 MiB guard")

    region = json.loads((root / "logo-roi.json").read_text(encoding="utf-8-sig"))
    box = tuple(int(region[k]) for k in ("x", "y")) + (
        int(region["x"] + region["width"]), int(region["y"] + region["height"])
    )
    final = list(Image.open(root / "fade-700.png").convert("RGB").crop(box).get_flattened_data())
    mask = [i for i, p in enumerate(final) if p[0] > 60 and p[2] > p[1] * 1.35 and p[2] - p[1] > 35]
    if not mask:
        raise AssertionError("Credits logo was not found in the final frame")
    frames = json.loads((root / "fade-times.json").read_text(encoding="utf-8-sig"))
    early = min((f for f in frames if 80 <= f["actual"] <= 220), key=lambda f: abs(f["actual"] - 150), default=None)
    if early is None:
        raise AssertionError("No early capture; rerun with the app foreground and animations enabled")
    pixels = list(Image.open(root / f"fade-{early['target']}.png").convert("RGB").crop(box).get_flattened_data())
    strength = sum(max(0, pixels[i][2] - pixels[i][1]) for i in mask)
    reference = sum(max(0, final[i][2] - final[i][1]) for i in mask)
    ratio = strength / reference
    print(f"Credits logo strength at {early['actual']} ms: {ratio:.1%}")
    if ratio > 0.65:
        failures.append("credits are already near full strength during the early fade frame")
    if failures:
        raise AssertionError("; ".join(failures))
    print("PASS: local navigation-growth and visible-fade regression checks")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    check(parser.parse_args().directory)
