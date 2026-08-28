"""Falsification suite untuk `antikythera_agent.server.component` (unit U22, D4).

Sumber kebenaran:
- contracts/shared/wire_protocol.golden.json  (entry `component_manifest`)
- documentation/WIRE_PROTOCOL.md  §2.6 (manifest + MIME)
- documentation/DECISIONS_RUNTIME_BRIDGE.md  D1/D4 (bundle = package data;
  extension additive; traversal dilarang)

Amplop test (asumsi yang dideklarasikan agar sertifikasi tetap sah):
- Bundle nyata `python/antikythera_agent/component/` dihasilkan oleh pipeline
  build (D1) dan belum tentu ada di source tree; test konten/MIME/traversal
  memakai fixture bundle sementara di tmp_path.
- `resolve` menerima path relatif yang SUDAH di-decode transport (U31);
  test tidak menuntut kode status HTTP — pemetaan None → 404/403 adalah
  keputusan transport.
- MIME hanya dikontrak untuk `.js`/`.wasm` (D4); ekstensi lain memakai
  fallback `application/octet-stream` (default binary RFC 2046).

Menjalankan (dari repo root):
    $env:PYTHONPATH="python"
    python -m pytest python/tests/test_component.py -v
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

import antikythera_agent
from antikythera_agent.server import component as component_module
from antikythera_agent.server.component import ComponentServer
from antikythera_agent.utils import get_version

_GOLDEN_PATH = (
    Path(__file__).resolve().parents[3] / "contracts" / "shared" / "wire_protocol.golden.json"
)

GOLDEN = json.loads(_GOLDEN_PATH.read_text(encoding="utf-8"))

ENTRY_NAME = "antikythera-sdk.js"
WASM_NAME = "antikythera-sdk.runner.core.wasm"
JS_BYTES = b'export const sdk = "test";\n'
WASM_BYTES = b"\x00asm\x01\x00\x00\x00"

#: Path request yang mencoba keluar dari bundle_dir — semua wajib → None.
TRAVERSAL_PATHS = [
    "../escape.js",
    "..",
    "..\\escape.js",
    "a/..",
    "a/../escape.js",
    "a/../../escape.js",
    "a\\..\\escape.js",
    "/etc/passwd",
    "/absolute.js",
    "\\absolute.js",
    "C:/outside/escape.js",
    "C:\\outside\\escape.js",
    "antikythera-sdk.js/../escape.js",
    "a//escape.js",
    "a/./escape.js",
    "escape.js/",
]


@pytest.fixture
def bundle_dir(tmp_path: Path) -> Path:
    d = tmp_path / "component"
    d.mkdir()
    (d / ENTRY_NAME).write_bytes(JS_BYTES)
    (d / WASM_NAME).write_bytes(WASM_BYTES)
    (d / "notes.txt").write_bytes(b"plain text")
    return d


@pytest.fixture
def server(bundle_dir: Path) -> ComponentServer:
    return ComponentServer(bundle_dir=bundle_dir)


# ===========================================================================
# Konstanta modul — kunci yang dipakai U31 untuk memasang route
# ===========================================================================

def test_module_constants_match_golden_component_manifest():
    """Konstanta route/BASE/ENTRY sama persis dengan nilai kanonik golden."""
    golden = GOLDEN["component_manifest"]
    assert component_module.BASE_PATH == golden["base"]
    assert component_module.ENTRY == golden["entry"]
    assert component_module.MANIFEST_PATH == golden["base"] + "manifest"


# ===========================================================================
# manifest — shape persis golden; nol field di luar golden
# ===========================================================================

def test_manifest_shape_persis_golden(server):
    """manifest: objek identik dengan sampel golden `component_manifest`
    (invarian 5 — tanpa kunci tambahan maupun kunci yang hilang)."""
    m = server.manifest()
    golden = GOLDEN["component_manifest"]
    assert isinstance(m, dict)
    assert set(m.keys()) == set(golden.keys())
    assert m == golden


def test_manifest_version_derived_from_sdk_version(server):
    """manifest: `version` diambil dari versi SDK (get_version/__version__),
    bukan konstanta terpisah yang bisa melenceng dari paket."""
    version = server.manifest()["version"]
    assert version == get_version()
    assert version == antikythera_agent.__version__


# ===========================================================================
# resolve — file yang ada; konten as-is + MIME terdaftar
# ===========================================================================

def test_resolve_known_js_entry_returns_bytes_and_js_mime(server):
    """resolve: entry JS dikembalikan as-is dengan MIME `text/javascript`."""
    assert server.resolve(ENTRY_NAME) == (JS_BYTES, "text/javascript")


def test_resolve_known_wasm_returns_bytes_and_wasm_mime(server):
    """resolve: file wasm dikembalikan as-is dengan MIME `application/wasm`."""
    assert server.resolve(WASM_NAME) == (WASM_BYTES, "application/wasm")


def test_resolve_known_nested_path_served_verbatim(bundle_dir):
    """resolve: path relatif bersarang di dalam root dilayani (layout bundle
    adalah tanggung jawab server — D4; klien tidak boleh meng-hardcode)."""
    (bundle_dir / "sub").mkdir()
    (bundle_dir / "sub" / "chunk.js").write_bytes(b"nested")
    server = ComponentServer(bundle_dir=bundle_dir)
    assert server.resolve("sub/chunk.js") == (b"nested", "text/javascript")


def test_resolve_unknown_extension_uses_octet_stream_fallback(server):
    """resolve: ekstensi tak terdaftar (mis. .d.ts) tetap dilayani as-is
    dengan fallback `application/octet-stream` (amplop dideklarasikan)."""
    assert server.resolve("notes.txt") == (b"plain text", "application/octet-stream")


# ===========================================================================
# resolve — tidak ada / input kosong
# ===========================================================================

def test_resolve_missing_file_returns_none(server):
    """resolve: file yang tidak ada di dalam bundle → None."""
    assert server.resolve("missing.js") is None


def test_resolve_empty_path_returns_none(server):
    """resolve: path kosong → None (tidak menunjuk file apa pun)."""
    assert server.resolve("") is None


# ===========================================================================
# resolve — traversal guard tertutup
# ===========================================================================

@pytest.mark.parametrize("path", TRAVERSAL_PATHS, ids=lambda v: v)
def test_resolve_rejects_traversal_attempts(server, tmp_path, path):
    """resolve: setiap upaya keluar dari bundle_dir (`..`, absolute, backslash,
    komponen kosong/`.`) → None; file rahasia di luar root TIDAK terbaca."""
    secret = tmp_path / "secret.js"
    secret.write_bytes(b"secret")
    assert server.resolve(path) is None


def test_resolve_rejects_symlink_escaping_bundle(tmp_path):
    """resolve: symlink di dalam bundle yang menunjuk keluar root ditolak oleh
    lapisan kontainmen resolve()-is_relative_to (guard lapis kedua)."""
    bundle = tmp_path / "component"
    bundle.mkdir()
    secret = tmp_path / "secret.js"
    secret.write_bytes(b"secret")
    try:
        (bundle / "link.js").symlink_to(secret)
    except OSError:
        pytest.skip("symlink creation not permitted on this platform")
    server = ComponentServer(bundle_dir=bundle)
    assert server.resolve("link.js") is None


# ===========================================================================
# is_known_entry
# ===========================================================================

def test_is_known_entry_true_for_manifest_entry(server):
    """is_known_entry: entry manifest yang ada di bundle → True."""
    assert server.is_known_entry(ENTRY_NAME) is True


def test_is_known_entry_false_for_missing_and_traversal(server):
    """is_known_entry: file hilang dan path traversal → False."""
    assert server.is_known_entry("missing.js") is False
    assert server.is_known_entry("../escape.js") is False


# ===========================================================================
# Constructor default — bundle package data (lazy, D1)
# ===========================================================================

def test_default_bundle_dir_is_package_component_directory():
    """Constructor default: bundle_dir = python/antikythera_agent/component/."""
    server = ComponentServer()
    expected = (Path(antikythera_agent.__file__).resolve().parent / "component").resolve()
    assert server.bundle_dir == expected


def test_default_constructor_is_lazy_when_bundle_absent():
    """Constructor default: tidak melakukan I/O (lazy) — manifest tetap
    menjawab shape golden meski bundle belum dihasilkan pipeline build."""
    server = ComponentServer()
    assert server.manifest() == GOLDEN["component_manifest"]
