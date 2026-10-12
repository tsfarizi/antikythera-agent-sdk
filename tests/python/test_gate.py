"""Falsification suite untuk `antikythera_agent.server.gate` (unit U13, E13).

Peran Coder — verifikasi mekanis wajib sebelum deklarasi selesai. Suite ini
menguji kontrak sambungan E13 yang dikonsumsi unit hilir (U31 transport,
U21 control, U23 loop_owner):

- Default-DENY: tool tidak boleh dieksekusi kecuali di-allowlist; hook
  tidak boleh dipanggil kecuali di-allow.
- Semua denial WAJIB berupa error berprefix `permission:` (repo invariant
  R4) — tidak ada degradasi senyap; test hilir memeriksa
  `startswith("permission:")`.
- Policy per destination `{server, client, mcp}` dan per hook name.
- Mekanisme deteksi denial: `PermissionDeniedError` (str/`.message`
  berprefix `permission:`) untuk transport HTTP 403 dan postback ok=false.

Sumber kebenaran perilaku:
- npm/antikythera-sdk/runtime/policy.js (JS gate: deny default, boolean
  `allows`, pesan `permission: tool 'X' not in allowlist`)
- antikythera-server-runtime/src/config.rs (Rust GatePolicy: allowlist per
  destination, pesan `permission: hook 'X' not allowed`, deny-all posture)
- contracts/shared/wire_protocol.golden.json (`postback_gate_denial` dan
  `error_event` memakai pesan `permission: tool 'rm' not in allowlist`)

Amplop test:
- Klausa "denial" ditangkap sebagai `Exception` umum (jenis exception
  tidak dikontrak oleh E13) ATAU sebagai `PermissionDeniedError` khusus
  (mekanisme deteksi yang dikontrak U31).
- Nama destination persis `server`/`client`/`mcp`; destination lain
  adalah program error (ValueError), bukan denial.

Menjalankan (dari repo root):
    $env:PYTHONPATH="python"
    python -m pytest python/tests/test_gate.py -v

Default deny — invariant R4: denial berprefix `permission:`.

Mekanisme deteksi denial (konsumsi U31 transport / U21 control).

Entry-point guard — destination tak dikenal adalah program error.
"""

from __future__ import annotations

import pytest

from antikythera_agent.server.gate import (
    PERMISSION_PREFIX,
    PermissionDeniedError,
    PolicyGate,
)

DESTINATIONS = ("server", "client", "mcp")
HOOK_NAMES = ("prepare-turn", "decide-action", "handle-tool-result")



@pytest.mark.parametrize("destination", DESTINATIONS)
def test_default_gate_denies_every_tool_with_permission_prefix(destination):
    """Default deny: gate baru menolak tool apa pun di semua destination."""
    gate = PolicyGate()
    with pytest.raises(Exception) as exc:
        gate.check_tool(destination, "echo")
    assert str(exc.value).startswith("permission:"), (
        f"denial must start with 'permission:', got {str(exc.value)!r}"
    )


@pytest.mark.parametrize("hook", HOOK_NAMES)
def test_default_gate_denies_every_hook_with_permission_prefix(hook):
    """Default deny: gate baru menolak hook apa pun."""
    gate = PolicyGate()
    with pytest.raises(Exception) as exc:
        gate.check_hook(hook)
    assert str(exc.value).startswith("permission:"), (
        f"denial must start with 'permission:', got {str(exc.value)!r}"
    )


def test_denial_prefix_is_exactly_lowercase_permission_colon():
    """Prefix denial persis `permission:` (lowercase + titik dua) — test
    hilir memeriksa `startswith("permission:")`."""
    gate = PolicyGate()
    with pytest.raises(Exception) as exc:
        gate.check_tool("client", "rm")
    msg = str(exc.value)
    assert msg.startswith("permission:")
    assert msg.startswith(PERMISSION_PREFIX)
    assert msg == "permission: tool 'rm' not in allowlist"



def test_allow_grants_only_listed_tool():
    """Allowlist memberi hak hanya pada tool yang terdaftar; lainnya tetap
    ditolak dengan pesan acuan persis."""
    gate = PolicyGate()
    gate.allow_tool("server", "echo")
    gate.check_tool("server", "echo")
    with pytest.raises(Exception) as exc:
        gate.check_tool("server", "rm")
    assert str(exc.value) == "permission: tool 'rm' not in allowlist"


def test_allow_hook_grants_hook_and_keeps_others_denied():
    """allow_hook memberi hak pada satu hook; hook lain tetap ditolak."""
    gate = PolicyGate()
    gate.allow_hook("prepare-turn")
    gate.check_hook("prepare-turn")
    with pytest.raises(Exception) as exc:
        gate.check_hook("decide-action")
    assert str(exc.value) == "permission: hook 'decide-action' not allowed"



def test_allowlist_is_per_destination():
    """Allow tool di satu destination tidak memberi hak di destination lain."""
    gate = PolicyGate()
    gate.allow_tool("server", "echo")
    gate.check_tool("server", "echo")
    for other in ("client", "mcp"):
        with pytest.raises(Exception) as exc:
            gate.check_tool(other, "echo")
        assert str(exc.value).startswith("permission:"), (
            f"tool allowed on server must be denied on {other}"
        )


def test_allows_boolean_is_per_destination():
    """allows (mirror JS) mencerminkan isolasi per destination."""
    gate = PolicyGate()
    gate.allow_tool("server", "echo")
    assert gate.allows("server", "echo") is True
    assert gate.allows("client", "echo") is False
    assert gate.allows("mcp", "echo") is False



def test_allows_hook_boolean_mirrors_grant():
    """allows_hook mengikuti allowlist hook."""
    gate = PolicyGate()
    assert gate.allows_hook("prepare-turn") is False
    gate.allow_hook("prepare-turn")
    assert gate.allows_hook("prepare-turn") is True
    assert gate.allows_hook("decide-action") is False


def test_hook_denial_message_matches_reference():
    """Pesan denial hook persis acuan Rust: `permission: hook 'X' not allowed`."""
    gate = PolicyGate()
    with pytest.raises(PermissionDeniedError) as exc:
        gate.check_hook("handle-tool-result")
    assert str(exc.value) == "permission: hook 'handle-tool-result' not allowed"



def test_denial_raises_permission_denied_error_with_message():
    """Denial menimbulkan `PermissionDeniedError`; `str` dan `.message`
    berprefix `permission:` — mekanisme deteksi tanpa parsing string."""
    gate = PolicyGate()
    with pytest.raises(PermissionDeniedError) as exc:
        gate.check_tool("server", "rm")
    err = exc.value
    assert str(err).startswith("permission:")
    assert err.message == "permission: tool 'rm' not in allowlist"



def test_default_allowlist_snapshots_are_empty():
    """Snapshot gate baru kosong di semua destination (posture deny-all)."""
    gate = PolicyGate()
    for destination in DESTINATIONS:
        assert gate.allowed_tools(destination) == frozenset()
    assert gate.allowed_hooks() == frozenset()


def test_snapshot_allowed_tools_is_frozen_copy():
    """Snapshot tool immutable dan terisolasi: perubahan gate setelah
    snapshot maupun destination lain tidak mengubah snapshot.


    Immutable — snapshot bukan jendela mutasi gate.
    """
    gate = PolicyGate()
    gate.allow_tool("server", "echo")
    snap = gate.allowed_tools("server")
    assert snap == frozenset({"echo"})
    assert isinstance(snap, frozenset)
    with pytest.raises(AttributeError):
        snap.add("x")
    gate.allow_tool("server", "rm")
    assert snap == frozenset({"echo"})
    assert gate.allowed_tools("client") == frozenset()


def test_snapshot_allowed_hooks_is_frozen_copy():
    """Snapshot hook immutable dan terisolasi dari mutasi gate lanjutan."""
    gate = PolicyGate()
    gate.allow_hook("prepare-turn")
    snap = gate.allowed_hooks()
    assert snap == frozenset({"prepare-turn"})
    with pytest.raises(AttributeError):
        snap.add("x")
    gate.allow_hook("decide-action")
    assert snap == frozenset({"prepare-turn"})



@pytest.mark.parametrize("bad", ["", "remote", "SERVER", None])
def test_unknown_destination_is_program_error_not_denial(bad):
    """Destination di luar {server, client, mcp} ditolak sebagai ValueError
    (program error), bukan denial `permission:` — menyamarkan bug menjadi
    denial melanggar R4 (prefix hanya untuk denial nyata)."""
    gate = PolicyGate()
    with pytest.raises(ValueError):
        gate.check_tool(bad, "echo")
    with pytest.raises(ValueError):
        gate.allow_tool(bad, "echo")
    with pytest.raises(ValueError):
        gate.allowed_tools(bad)


def test_non_string_tool_name_rejected_at_allow():
    """Nama non-str ditolak di titik masuk konfigurasi: membiarkannya masuk
    allowlist menciptakan state yang tidak akan pernah match lookup (degradasi
    senyap di sisi operasional)."""
    gate = PolicyGate()
    with pytest.raises(TypeError):
        gate.allow_tool("server", 42)
    with pytest.raises(TypeError):
        gate.allow_hook(42)
