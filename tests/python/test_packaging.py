"""Packaging gates: PEP 561 typing marker and built-distribution data files.

(a) py.typed and __init__.pyi must sit beside __init__.py so type checkers
    treat the package as inline-typed (PEP 561; both are declared in
    pyproject package-data).
(b) When `build` is installed, a wheel is produced into a tmp dir and the
    three data artifacts declared in [tool.setuptools.package-data]
    (py.typed, __init__.pyi, antikythera.wasm) must be present inside it.
"""

import importlib.util
import subprocess
import sys
import zipfile
from pathlib import Path

import pytest

PACKAGE_DIR = Path(__file__).resolve().parents[2] / "clients" / "python" / "antikythera_agent"
PYPROJECT_DIR = PACKAGE_DIR.parent


def test_pep561_marker_and_stub_exist() -> None:
    init = PACKAGE_DIR / "__init__.py"
    assert init.is_file(), f"missing {init}"
    py_typed = PACKAGE_DIR / "py.typed"
    stub = PACKAGE_DIR / "__init__.pyi"
    assert py_typed.is_file(), (
        f"missing {py_typed}: PEP 561 marker must exist next to __init__.py "
        "(declared in pyproject [tool.setuptools.package-data])"
    )
    assert stub.is_file(), f"missing {stub}: inline type stub must exist next to __init__.py"


def _build_is_available() -> bool:
    return importlib.util.find_spec("build") is not None


@pytest.mark.skipif(not _build_is_available(), reason="python -m build is not installed")
def test_wheel_carries_declared_data_files(tmp_path: Path) -> None:
    outdir = tmp_path / "dist"
    subprocess.run(
        [sys.executable, "-m", "build", "--wheel", "--no-isolation", "--outdir", str(outdir)],
        cwd=PYPROJECT_DIR,
        check=True,
        capture_output=True,
    )
    wheels = list(outdir.glob("*.whl"))
    assert wheels, "python -m build --wheel produced no wheel"
    names = set()
    with zipfile.ZipFile(wheels[0]) as whl:
        names.update(whl.namelist())
    for required in (
        "antikythera_agent/py.typed",
        "antikythera_agent/__init__.pyi",
        "antikythera_agent/antikythera.wasm",
    ):
        assert required in names, f"{required} missing from wheel {wheels[0].name}"
