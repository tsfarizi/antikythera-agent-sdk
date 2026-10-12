"""Debt gate: ensure no time-boxed debt markers remain.

All lint/type debt must be fixed directly — no Expiry markers allowed.
Scan: pyproject.toml, *.py, tsconfig.json, *.ts/*.d.ts
"""
from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

def scan_file(path: Path) -> list[str]:
    errors = []
    try:
        text = path.read_text(encoding="utf-8")
    except Exception:
        return errors
    for idx, line in enumerate(text.splitlines(), 1):
        if "TODO" in line and "Expiry:" in line:
            errors.append(f"{path}:{idx}: legacy time-boxed marker still present: {line.strip()}")
    return errors

def main() -> int:
    errors: list[str] = []
    errors += scan_file(ROOT / "pyproject.toml")
    for p in (ROOT / "antikythera_agent").rglob("*.py"):
        errors += scan_file(p)
    npm_root = ROOT.parent / "npm" / "antikythera-sdk"
    errors += scan_file(npm_root / "tsconfig.json")
    for p in npm_root.rglob("*.ts"):
        errors += scan_file(p)
    for p in npm_root.rglob("*.d.ts"):
        errors += scan_file(p)
    doc_root = ROOT.parent.parent / "documentation"
    for p in doc_root.rglob("*.md"):
        errors += scan_file(p)
    if errors:
        print(
            "Debt gate failed: legacy Expiry markers still present "
            "(must be fixed directly):",
            file=sys.stderr,
        )
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1
    print("Debt gate passed: no time-boxed markers, debt fixed directly")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
