r"""Falsification suite untuk higiene Python K1/K4 — fase RED (hanya gagal).

Sumber kebenaran:
- python/antikythera_agent/server/component.py:53-64 (bundle_dir default
  = antikythera_agent/component, ENTRY = "antikythera-sdk.js", manifest())
- python/antikythera_agent/utils.py (klausa K1: bebas F401)
- Desain U1: bila bundle hilang, manifest() atau resolve(ENTRY) WAJIB
  fail-fast RuntimeError dengan pesan mengandung "task build-python-wasm"
- Kontrak lint K1: utils.py tidak boleh mengimpor simbol yang tidak dipakai
  (F401) — per-file-ignores saat ini menyembunyikan utang.

Amplop test (asumsi yang dideklarasikan agar sertifikasi tetap sah):
- ComponentServer constructor lazy (tidak I/O) — kegagalan baru di manifest()
  / resolve() — test memalsifikasi klausa fail-fast, bukan constructor.
- K4: dua tepi amplop validitas bundle_hilang: (a) bundle_dir tidak ada di
  disk, (b) bundle_dir ada tapi kosong tanpa ENTRY.
- K1: lint-contract via subprocess `python -m ruff check ... --select F401`;
  bila ruff belum terpasang, test skip eksplisit (jangan fail) — deterministik
  dan terisolasi, tanpa mock runner.
- Status suite: sertifikasi higiene — utils.py sudah dibersihkan Coder, jadi
  kedua test WAJIB HIJAU; kegagalan di sini = regresi utang yang kembali.

Metode falsifikasi:
- K4: buktikan manifest() diam-diam mengembalikan dict walau bundle hilang
  (seharusnya raise RuntimeError) — perburuan batas: nonexistent vs empty dir.
- K1: sertifikasi klausa higiene "utils.py bebas dead-import" — lari
  `python -m ruff check python/antikythera_agent/utils.py --select F401`
  dan assert exit == 0 DAN output TIDAK mengandung F401. Ruff bersih =
  kontrak terpenuhi; F401 terdeteksi = klausa dilanggar.
- Satu klaim per test; deterministik & isolasi via tmp_path / subprocess.

Menjalankan (dari repo root):
    $env:PYTHONPATH="python"
    python -m pytest python/tests/test_uph_hygiene_red.py -q --basetemp="$env:TEMP\opencode\pyt" -p no:cacheprovider

K4 — ComponentServer.manifest fail-fast bila bundle hilang.

K1 — dead import utils.py (F401).
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

from antikythera_agent.server.component import ENTRY, ComponentServer



def test_k4_manifest_fail_fast_bila_bundle_hilang_wajib_raise_runtime_error_mengandung_task_build_python_wasm(tmp_path: Path):
    """Klausa K4: bila bundle_dir tidak ada ATAU ENTRY tidak ada di dalamnya,
    manifest() atau resolve(ENTRY) WAJIB raise RuntimeError dengan pesan
    mengandung "task build-python-wasm" (desain U1).

    Metode falsifikasi: dua tepi amplop validitas — (a) bundle_dir =
    Path(tmp_path)/nonexistent (tidak ada di disk), (b) bundle_dir = dir
    kosong tanpa ENTRY. Saat ini manifest() diam-diam mengembalikan dict
    walau bundle hilang → test RED (DID NOT RAISE).


    (a) bundle_dir tidak ada di disk.

    Sengaja tidak membuat direktori — menguji tepi "tidak ada".

    (b) bundle_dir ada tapi kosong tanpa ENTRY.

    Varian resolve(ENTRY) juga dikontrak fail-fast — bila implementasi memilih raise di
    resolve alih-alih manifest, klausa tetap menuntut salah satunya raise; test ini memperkuat
    falsifikasi via resolve.
    """
    nonexistent = tmp_path / "nonexistent_bundle"
    assert not nonexistent.exists()
    server_a = ComponentServer(bundle_dir=nonexistent)
    with pytest.raises(RuntimeError, match="task build-python-wasm"):
        server_a.manifest()

    empty = tmp_path / "empty_bundle"
    empty.mkdir()
    assert empty.is_dir()
    assert not (empty / ENTRY).exists()
    server_b = ComponentServer(bundle_dir=empty)
    with pytest.raises(RuntimeError, match="task build-python-wasm"):
        server_b.manifest()

    with pytest.raises(RuntimeError, match="task build-python-wasm"):
        server_a.resolve(ENTRY)  # type: ignore[func-returns-value]
    with pytest.raises(RuntimeError, match="task build-python-wasm"):
        server_b.resolve(ENTRY)  # type: ignore[func-returns-value]



def test_k1_utils_tidak_boleh_mengandung_dead_import_F401():
    """Klausa K1: python/antikythera_agent/utils.py tidak boleh mengimpor
    simbol yang tidak dipakai (F401) — "utils.py bebas dead-import".

    Metode sertifikasi: lint-contract via subprocess
    `python -m ruff check python/antikythera_agent/utils.py --select F401`
    dan assert exit == 0 DAN output TIDAK mengandung F401. Bila ruff belum
    terpasang, pytest.skip dengan alasan eksplisit (jangan fail). Ruff
    melaporkan F401 (exit != 0 / output memuat F401) = klausa dilanggar.


    Amplop: ruff belum terpasang → skip eksplisit, bukan fail.

    Fallback: bila executable ruff tidak ditemukan sama sekali.

    Klausa higiene K1: ruff check --select F401 HARUS exit 0 DAN output TIDAK mengandung F401
    — utils.py bebas dead-import. Pelanggaran apa pun (dead import terdeteksi, ATAU lint gagal
    karena alasan lain yang menyembunyikan kebersihan) mematahkan klausa ini.
    """
    result = subprocess.run(
        [sys.executable, "-m", "ruff", "check", "python/antikythera_agent/utils.py", "--select", "F401"],
        capture_output=True,
        text=True,
    )
    combined = (result.stdout or "") + (result.stderr or "")

    if "No module named ruff" in combined or "No module named" in combined:
        pytest.skip("ruff belum terpasang — lint-contract F401 tidak dapat dieksekusi di env ini")
    if result.returncode == 9009 or "not recognized" in combined.lower():
        pytest.skip("ruff belum terpasang — lint-contract F401 tidak dapat dieksekusi di env ini")

    assert result.returncode == 0 and "F401" not in combined, (
        f"klausa 'utils.py bebas dead-import' dilanggar: "
        f"`ruff check python/antikythera_agent/utils.py --select F401` "
        f"tidak bersih — exit={result.returncode}, output={combined!r}"
    )
