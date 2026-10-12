"""Falsification suite untuk `antikythera_agent.server.control` (unit U21).

Peran Coder — verifikasi mekanis WAJIB sebelum deklarasi selesai. Suite ini
menguji kontrak sambungan U21 yang dikonsumsi unit hilir (U31 transport:
registrasi SSE + POST-back dispatch + push llm-token/lifecycle; U23
loop_owner: create_correlation + await_postback + is_client_connected):

- Pendaftaran client SSE: satu client_id satu koneksi aktif; re-register
  menggantikan; unregister menghentikan keepalive/presence.
- Push envelope: frame `data: {...}

`; envelope shape golden `*_event`
  PERSIS lima kunci (invarian 5: nol field ekstra — ditolak, bukan di-drop).
- Korelasi + TTL: correlation id unik; POST-back unknown/expired diabaikan
  dan di-log (bukan error); expired fail-closed.
- Await respons: blok sampai POST-back tiba; timeout menimbulkan
  `PendingTimeoutError` berprefix `permission:` (fail-closed, WIRE_PROTOCOL
  §5 — no silent hang); batas tunggu = min(timeout_secs, sisa TTL).
- Keepalive periodik `: keepalive

` dan sweep TTL.

Sumber kebenaran perilaku:
- documentation/WIRE_PROTOCOL.md §2.4/§2.5/§3/§5
- contracts/shared/wire_protocol.golden.json (shape `*_event` dan
  `postback_*`)
- antikythera-server-runtime/src/control.rs (semantik register/replace,
  push->bool, unknown/expired diabaikan, pending_len)
- antikythera-server-runtime/src/http.rs (keepalive 15s, mismatch path
  diabaikan)
- npm/antikythera-sdk/runtime/sse.js (format frame)

Amplop test (asumsi yang dideklarasikan agar sertifikasi tetap sah):
- Writer di-inject sebagai callable `(bytes) -> None` perekam frame; tidak
  ada socket sungguhan.
- Uji timeout memakai batas kecil (0.05–0.1s) agar deterministik; klausa
  "elapsed" memakai headroom 10x untuk mesin CI yang lambat.
- Await dijalankan di thread terpisah (transport thread-per-connection);
  exception ditangkap ke container hasil.

Menjalankan (dari repo root):
    $env:PYTHONPATH="python"
    python -m pytest python/tests/test_control.py -v

Pendaftaran client: satu client_id satu koneksi aktif (contract 1, 5, 6).

Push: frame SSE + envelope shape golden (contract 2).

Keepalive lifecycle (contract 2, 6).

Korelasi + TTL (contract 3).

Konkurensi — transport ThreadingHTTPServer thread-per-connection.
"""

from __future__ import annotations

import json
import logging
import threading
import time
from pathlib import Path

import pytest

from antikythera_agent.server.control import (
    ENVELOPE_KEYS,
    KEEPALIVE_FRAME,
    ControlChannel,
    PendingTimeoutError,
)

_GOLDEN_PATH = (
    Path(__file__).resolve().parents[2] / "contracts" / "shared" / "wire_protocol.golden.json"
)

GOLDEN = json.loads(_GOLDEN_PATH.read_text(encoding="utf-8"))

#: Kunci kanonik POST-back (golden `postback_response` / `postback_gate_denial`).
POSTBACK_KEYS = {"correlation_id", "ok", "payload", "error"}

#: Semua shape event golden yang memakai envelope 5-kunci.
GOLDEN_EVENT_SHAPES = [
    "tool_execution_request_event",
    "hook_request_event",
    "llm_token_event",
    "registry_sync_event",
    "lifecycle_event",
    "error_event",
]


class _RecordingWriter:
    """Writer callable `(bytes) -> None` yang merekam frame untuk asersi."""

    def __init__(self) -> None:
        self.frames: list[bytes] = []

    def __call__(self, data: bytes) -> None:
        self.frames.append(data)


def _await_thread(control, correlation_id, timeout_secs, results, index):
    """Jalankan await_postback di thread terpisah; tangkap exception."""

    def run():
        try:
            results[index] = ("ok", control.await_postback(correlation_id, timeout_secs))
        except Exception as exc:  # noqa: BLE001 - semua kegagalan ditangkap test
            results[index] = ("error", exc)

    thread = threading.Thread(target=run, daemon=True)
    thread.start()
    return thread


def _wait_for(predicate, timeout_secs=1.0, step=0.005):
    """Polling deterministik sampai predicate True atau timeout habis."""
    deadline = time.monotonic() + timeout_secs
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(step)
    return predicate()


def _parse_frame(frame: bytes) -> dict:
    """Parse frame SSE `data: {json}\\n\\n` menjadi dict."""
    assert frame.startswith(b"data: "), frame
    assert frame.endswith(b"\n\n"), frame
    return json.loads(frame[len(b"data: ") : -2])



def test_register_client_marks_connected_and_isolates_ids():
    """register_client: presence menyala hanya untuk id yang terdaftar."""
    control = ControlChannel()
    assert control.is_client_connected("client-a") is False
    control.register_client("client-a", _RecordingWriter())
    assert control.is_client_connected("client-a") is True
    assert control.is_client_connected("client-b") is False


def test_unregister_client_clears_presence_and_is_idempotent():
    """unregister_client: presence padam; pemanggilan ulang False (idempotent)."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    assert control.unregister_client("client-a") is True
    assert control.is_client_connected("client-a") is False
    assert control.unregister_client("client-a") is False


def test_re_register_replaces_writer_one_active_connection():
    """Satu client_id satu koneksi aktif: re-register menggantikan writer lama
    (mirror Rust insert) — client lama tidak menerima push berikutnya."""
    control = ControlChannel()
    writer_old = _RecordingWriter()
    writer_new = _RecordingWriter()
    control.register_client("client-a", writer_old)
    control.register_client("client-a", writer_new)
    control.push("client-a", dict(GOLDEN["lifecycle_event"]))
    assert len(writer_new.frames) == 1
    assert len(writer_old.frames) == 0


def test_register_client_rejects_invalid_inputs():
    """Entry-point guard: client_id kosong/non-str dan writer non-callable
    ditolak sebelum menyentuh state (invariance enforcement)."""
    control = ControlChannel()
    with pytest.raises(ValueError):
        control.register_client("", _RecordingWriter())
    with pytest.raises(ValueError):
        control.register_client(None, _RecordingWriter())
    with pytest.raises(TypeError):
        control.register_client("client-a", "not-callable")



def test_push_delivers_golden_frame_to_registered_client():
    """push: client terdaftar menerima frame `data: {json}\\n\\n` yang JSON-nya
    round-trip identik dengan envelope golden."""
    control = ControlChannel()
    writer = _RecordingWriter()
    control.register_client("client-a", writer)
    envelope = GOLDEN["tool_execution_request_event"]
    assert control.push("client-a", envelope) is True
    assert len(writer.frames) == 1
    assert _parse_frame(writer.frames[0]) == envelope


def test_push_frame_is_exact_sse_data_format():
    """Frame push PERSIS `data: {json}\\n\\n` — dua newline tunggal di akhir,
    tanpa kunci frame lain (event:/id:/retry:)."""
    control = ControlChannel()
    writer = _RecordingWriter()
    control.register_client("client-a", writer)
    control.push("client-a", dict(GOLDEN["lifecycle_event"]))
    frame = writer.frames[0]
    assert frame.startswith(b"data: ")
    assert frame.endswith(b"\n\n")
    assert frame.count(b"\n") == 2
    assert b"event:" not in frame
    assert b"id:" not in frame
    assert b"retry:" not in frame


def test_push_payload_with_newline_does_not_break_framing():
    """Payload ber-newline di-escape JSON sehingga frame tetap dua newline
    (framing SSE tidak bisa dipecah oleh konten payload)."""
    control = ControlChannel()
    writer = _RecordingWriter()
    control.register_client("client-a", writer)
    envelope = dict(GOLDEN["llm_token_event"])
    envelope["payload"] = {"session_id": "session-123", "chunk": "line1\nline2", "correlation_id": None}
    assert control.push("client-a", envelope) is True
    frame = writer.frames[0]
    assert frame.count(b"\n") == 2
    assert _parse_frame(frame) == envelope


@pytest.mark.parametrize(
    "shape_name", GOLDEN_EVENT_SHAPES, ids=GOLDEN_EVENT_SHAPES
)
def test_golden_event_shapes_use_exact_envelope_keys(shape_name):
    """Konformansi golden: setiap shape event kanonik memakai PERSIS lima
    kunci envelope yang dikontrak ENVELOPE_KEYS (nol field ekstra)."""
    assert set(GOLDEN[shape_name].keys()) == set(ENVELOPE_KEYS)


def test_push_rejects_extra_keys_beyond_golden():
    """Invarian 5: envelope dengan field DI LUAR golden DITOLAK keras
    (bukan di-drop) — kesalahan produsen tidak boleh lolos senyap."""
    control = ControlChannel()
    writer = _RecordingWriter()
    control.register_client("client-a", writer)
    bad = dict(GOLDEN["lifecycle_event"])
    bad["extra"] = True
    with pytest.raises(ValueError):
        control.push("client-a", bad)
    assert writer.frames == []


def test_push_rejects_missing_golden_keys():
    """Invarian 5: envelope dengan kunci golden HILANG ditolak keras."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    bad = dict(GOLDEN["lifecycle_event"])
    del bad["payload"]
    with pytest.raises(ValueError):
        control.push("client-a", bad)


def test_push_rejects_non_object_envelope():
    """push: envelope non-dict ditolak di titik masuk."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    with pytest.raises(ValueError):
        control.push("client-a", "not-an-envelope")
    with pytest.raises(ValueError):
        control.push("client-a", None)


def test_push_rejects_non_string_type():
    """Envelope `type` wajib string (mirror Rust `event_type: String`)."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    bad = dict(GOLDEN["lifecycle_event"])
    bad["type"] = 42
    with pytest.raises(ValueError):
        control.push("client-a", bad)


def test_push_to_unregistered_client_returns_false():
    """push ke client yang tidak terdaftar mengembalikan False (mirror Rust
    `push -> bool`) tanpa exception; tidak ada frame yang dikirim."""
    control = ControlChannel()
    assert control.push("client-a", dict(GOLDEN["lifecycle_event"])) is False


def test_push_serializes_unicode_payload_verbatim():
    """JSON UTF-8 verbatim (mirror serde_json/JSON.stringify): unicode payload
    round-trip utuh tanpa escape backslash-u."""
    control = ControlChannel()
    writer = _RecordingWriter()
    control.register_client("client-a", writer)
    envelope = dict(GOLDEN["llm_token_event"])
    envelope["payload"] = {"session_id": None, "chunk": "halo dunia — antikythera", "correlation_id": None}
    control.push("client-a", envelope)
    frame = writer.frames[0]
    assert _parse_frame(frame) == envelope
    assert "\\u" not in frame.decode("utf-8")



def test_keepalive_comment_sent_periodically():
    """Keepalive `: keepalive\\n\\n` dikirim periodik ke client terdaftar
    (WIRE_PROTOCOL §2.4; interval default 15s, dikonfigurasi kecil di test)."""
    control = ControlChannel(keepalive_interval=0.02)
    writer = _RecordingWriter()
    control.register_client("client-a", writer)
    assert KEEPALIVE_FRAME == b": keepalive\n\n"
    assert _wait_for(lambda: len(writer.frames) >= 1, timeout_secs=1.0)
    assert writer.frames[0] == KEEPALIVE_FRAME


def test_unregister_client_stops_keepalive():
    """unregister_client menghentikan keepalive client itu (contract 6)."""
    control = ControlChannel(keepalive_interval=0.02)
    writer = _RecordingWriter()
    control.register_client("client-a", writer)
    assert _wait_for(lambda: len(writer.frames) >= 1, timeout_secs=1.0)
    count_before = len(writer.frames)
    assert control.unregister_client("client-a") is True
    time.sleep(0.08)
    assert len(writer.frames) == count_before


def test_keepalive_interval_must_be_positive():
    """Interval keepalive non-positif ditolak (wait(0) = busy-loop)."""
    with pytest.raises(ValueError):
        ControlChannel(keepalive_interval=0)
    with pytest.raises(ValueError):
        ControlChannel(keepalive_interval=-1)



def test_create_correlation_returns_unique_ids_and_tracks_pending():
    """create_correlation: id unik antar panggilan dan tercatat pending."""
    control = ControlChannel()
    corr_a = control.create_correlation("client-a")
    corr_b = control.create_correlation("client-a")
    assert isinstance(corr_a, str) and corr_a
    assert corr_a != corr_b
    assert control.pending_len() == 2


def test_create_correlation_rejects_invalid_inputs():
    """Entry-point guard: client_id kosong dan TTL non-positif ditolak."""
    control = ControlChannel()
    with pytest.raises(ValueError):
        control.create_correlation("", ttl_secs=60)
    with pytest.raises(ValueError):
        control.create_correlation("client-a", ttl_secs=0)
    with pytest.raises(ValueError):
        control.create_correlation("client-a", ttl_secs=-5)


def test_resolve_unknown_postback_is_ignored_and_logged_not_error(caplog):
    """POST-back dengan correlation id tidak dikenal diabaikan dan DI-LOG
    (bukan error) — mirror Rust complete_postback -> false; §1/§5."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    with caplog.at_level(logging.WARNING, logger="antikythera_agent.server.control"):
        result = control.resolve_postback(
            "never-created",
            {"correlation_id": "never-created", "ok": True, "payload": {}, "error": None},
        )
    assert result is False
    assert control.pending_len() == 0
    assert any(
        "unknown correlation" in r.getMessage() and "never-created" in r.getMessage()
        for r in caplog.records
    )


def test_resolve_expired_postback_is_ignored_fail_closed():
    """TTL expiry: POST-back yang tiba setelah deadline diabaikan (False),
    fail-closed — waiter sudah/bakal gagal sendiri (WIRE_PROTOCOL §5)."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    corr = control.create_correlation("client-a", ttl_secs=0.05)
    time.sleep(0.1)
    assert control.resolve_postback(
        corr, {"correlation_id": corr, "ok": True, "payload": {"late": True}, "error": None}
    ) is False
    assert control.pending_len() == 0


def test_resolve_duplicate_postback_second_ignored():
    """Satu correlation hanya bisa di-resolve sekali; POST-back duplikat
    diabaikan (mirror Rust: pending dihapus pada delivery pertama)."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    corr = control.create_correlation("client-a", ttl_secs=60)
    body = {"correlation_id": corr, "ok": True, "payload": {"v": 1}, "error": None}
    assert control.resolve_postback(corr, body) is True
    assert control.resolve_postback(corr, body) is False


def test_resolve_postback_body_correlation_mismatch_ignored():
    """POST-back yang body correlation_id-nya tidak cocok dengan path
    diabaikan dan TIDAK mengonsumsi pending (mirror http.rs mismatch path)."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    corr = control.create_correlation("client-a", ttl_secs=60)
    assert control.resolve_postback(
        corr, {"correlation_id": "other-corr", "ok": True, "payload": {}, "error": None}
    ) is False
    assert control.pending_len() == 1
    assert control.resolve_postback(
        corr, {"correlation_id": corr, "ok": True, "payload": {"v": 2}, "error": None}
    ) is True


def test_resolve_postback_normalizes_body_to_golden_shape():
    """Body POST-back dinormalisasi via wire.build_postback: hasil PERSIS
    empat kunci golden; `ok` strict boolean (mirror `=== true`)."""
    control = ControlChannel()
    writer = _RecordingWriter()
    control.register_client("client-a", writer)
    corr = control.create_correlation("client-a", ttl_secs=60)
    results = {}
    thread = _await_thread(control, corr, 5.0, results, 0)
    assert control.resolve_postback(
        corr,
        {"correlation_id": corr, "ok": "true", "payload": {"v": 1}, "error": None, "junk": 1},
    ) is True
    thread.join(timeout=2.0)
    assert results[0][0] == "ok"
    assert set(results[0][1].keys()) == POSTBACK_KEYS
    assert results[0][1] == {"correlation_id": corr, "ok": False, "payload": {"v": 1}, "error": None}


def test_resolve_postback_rejects_non_object_body():
    """Body POST-back non-dict adalah pelanggaran kontrak transport → ValueError."""
    control = ControlChannel()
    with pytest.raises(ValueError):
        control.resolve_postback("corr-x", "not-an-object")



def test_await_postback_returns_postback_body_on_success():
    """await_postback: blok sampai POST-back tiba; mengembalikan body hasil
    normalisasi (caller memutuskan makna ok/payload/error)."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    corr = control.create_correlation("client-a", ttl_secs=60)
    results = {}
    thread = _await_thread(control, corr, 5.0, results, 0)
    time.sleep(0.02)
    assert control.resolve_postback(
        corr,
        {"correlation_id": corr, "ok": True, "payload": {"tool-name": "t", "success": True}, "error": None},
    ) is True
    thread.join(timeout=2.0)
    assert results[0][0] == "ok"
    assert results[0][1]["ok"] is True
    assert results[0][1]["payload"] == {"tool-name": "t", "success": True}
    assert control.pending_len() == 0


def test_await_postback_succeeds_when_resolved_before_await():
    """Resolve sebelum await (POST-back cepat) tetap dikembalikan — mirror
    Rust oneshot yang menyimpan nilai sebelum receiver di-poll."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    corr = control.create_correlation("client-a", ttl_secs=60)
    body = {"correlation_id": corr, "ok": True, "payload": {"fast": True}, "error": None}
    assert control.resolve_postback(corr, body) is True
    assert control.await_postback(corr, 5.0) == body
    assert control.pending_len() == 0


def test_await_postback_timeout_raises_fail_closed_error():
    """Timeout: error eksplisit PendingTimeoutError berprefix `permission:`
    (fail-closed §5 — no silent hang); pending dihapus."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    corr = control.create_correlation("client-a", ttl_secs=60)
    results = {}
    thread = _await_thread(control, corr, 0.1, results, 0)
    thread.join(timeout=2.0)
    assert not thread.is_alive()
    assert results[0][0] == "error"
    assert isinstance(results[0][1], PendingTimeoutError)
    assert str(results[0][1]).startswith("permission:")
    assert corr in str(results[0][1])
    assert control.pending_len() == 0


def test_await_postback_fails_at_ttl_before_caller_timeout():
    """Batas tunggu = min(timeout_secs, sisa TTL): correlation yang TTL-nya
    lewat membuat waiter gagal fail-closed JAUH sebelum timeout_secs caller
    (expired tidak pernah diterima — WIRE_PROTOCOL §5).


    Gagal di batas TTL (~50ms), bukan di 5s.
    """
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    corr = control.create_correlation("client-a", ttl_secs=0.05)
    results = {}
    start = time.monotonic()
    thread = _await_thread(control, corr, 5.0, results, 0)
    thread.join(timeout=2.0)
    elapsed = time.monotonic() - start
    assert not thread.is_alive()
    assert results[0][0] == "error"
    assert isinstance(results[0][1], PendingTimeoutError)
    assert elapsed < 1.0
    assert control.pending_len() == 0


def test_await_postback_on_unknown_correlation_raises():
    """await pada correlation yang tidak dikenal = program error (ValueError)."""
    control = ControlChannel()
    with pytest.raises(ValueError):
        control.await_postback("never-created", 0.1)


def test_cancel_pending_removes_entry_and_blocks_late_resolve():
    """cancel_pending (path fail-closed caller: client tidak terhubung)
    menghapus pending; resolve setelahnya diabaikan; await setelahnya
    program error."""
    control = ControlChannel()
    corr = control.create_correlation("client-a", ttl_secs=60)
    assert control.cancel_pending(corr) is True
    assert control.pending_len() == 0
    assert control.cancel_pending(corr) is False
    assert control.resolve_postback(
        corr, {"correlation_id": corr, "ok": True, "payload": {}, "error": None}
    ) is False
    with pytest.raises(ValueError):
        control.await_postback(corr, 0.1)


def test_await_postback_returns_ok_false_denial_body_not_raised():
    """POST-back gate denial (`ok=false`, error `permission: ...`) DIKEMBALIKAN
    sebagai data, bukan di-raise: caller (U23) yang memutuskan menyerapnya
    sebagai error envelope (mirror Rust `if !body.ok { return Err(...) }`)."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    corr = control.create_correlation("client-a", ttl_secs=60)
    results = {}
    thread = _await_thread(control, corr, 5.0, results, 0)
    assert control.resolve_postback(
        corr,
        {"correlation_id": corr, "ok": False, "payload": None, "error": "permission: tool 'rm' not in allowlist"},
    ) is True
    thread.join(timeout=2.0)
    assert results[0][0] == "ok"
    assert results[0][1]["ok"] is False
    assert results[0][1]["error"].startswith("permission:")



def test_concurrent_roundtrips_are_thread_safe():
    """16 roundtrip korelasi paralel (await thread + resolve) semuanya
    tuntas dengan body yang cocok — state pending aman di bawah lock."""
    control = ControlChannel()
    control.register_client("client-a", _RecordingWriter())
    n = 16
    corrs = [control.create_correlation("client-a", ttl_secs=10) for _ in range(n)]
    results = {}
    threads = [
        _await_thread(control, corr, 5.0, results, i) for i, corr in enumerate(corrs)
    ]
    for i, corr in enumerate(corrs):
        control.resolve_postback(
            corr, {"correlation_id": corr, "ok": True, "payload": {"i": i}, "error": None}
        )
    for thread in threads:
        thread.join(timeout=2.0)
    assert not any(thread.is_alive() for thread in threads)
    for i, corr in enumerate(corrs):
        assert results[i][0] == "ok"
        assert results[i][1]["correlation_id"] == corr
        assert results[i][1]["payload"] == {"i": i}
    assert control.pending_len() == 0
