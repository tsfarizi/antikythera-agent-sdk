"""Control channel: SSE server→client + korelasi POST-back pending (unit U21).

Sumber kebenaran perilaku:
- documentation/WIRE_PROTOCOL.md  §2.4 (SSE control channel; envelope event
  `{type, correlation_id, session_id, client_id, payload}`; keepalive
  `: keepalive` interval default 15s), §2.5 (POST-back shape
  `{correlation_id, ok, payload, error}`), §3 (keluarga event), §5
  (correlation id TTL default 60s; POST-back unknown diabaikan + di-log;
  request tanpa POST-back dalam TTL = error eksplisit di sisi core
  `permission:`-style fail-closed, tanpa silent hang).
- contracts/shared/wire_protocol.golden.json  (shape kanonik `*_event` dan
  `postback_*`; invarian nol field di luar golden — WIRE_PROTOCOL §7).
- antikythera-server-runtime/src/control.rs  (semantik: register/replace;
  `push -> bool`; pending dihapus saat selesai; unknown/expired POST-back
  diabaikan + di-log; `pending_len` debug aid).
- antikythera-server-runtime/src/http.rs      (keepalive 15s; POST-back
  mismatch path vs body diabaikan + di-log).
- npm/antikythera-sdk/runtime/sse.js          (format frame `data: ...`).

Amplop runtime:
- Writer transport adalah callable `(bytes) -> None`; kegagalannya dianggap
  koneksi SSE mati dan client di-unregister (fail-closed: is_client_connected
  menjadi False; client re-register pada reconnect — WIRE_PROTOCOL §5).
- Keepalive dikirim dari SATU thread daemon per ControlChannel; writer yang
  memblok akan menahan keepalive client lain (batas transport: writer wajib
  non-blocking atau transport membatasi send-nya).
- Envelope outgoing TIDAK dinormalisasi melainkan DITOLAK bila menyimpang
  dari lima kunci golden (kesalahan produsen ditolak keras); POST-back
  incoming dinormalisasi via `wire.build_postback` (shape golden persis,
  drop-extra defensif mirror JS buildPostback).
"""

from __future__ import annotations

import itertools
import json
import logging
import threading
import time
import uuid
from typing import Any, Callable, Dict, Optional

from .wire import build_postback

logger = logging.getLogger(__name__)

#: Kunci kanonik envelope event (golden `*_event`) — PERSIS lima kunci,
#: tanpa kunci di luar golden (invarian 5, WIRE_PROTOCOL §7).
ENVELOPE_KEYS = frozenset({"type", "correlation_id", "session_id", "client_id", "payload"})

#: Frame keepalive SSE (WIRE_PROTOCOL §2.4; mirror axum KeepAlive text).
KEEPALIVE_FRAME = b": keepalive\n\n"

#: Interval keepalive default detik (WIRE_PROTOCOL §2.4 / http.rs 15s).
DEFAULT_KEEPALIVE_INTERVAL = 15.0

#: TTL default detik untuk correlation id (WIRE_PROTOCOL §5).
DEFAULT_CORRELATION_TTL = 60.0

_keepalive_seq = itertools.count(1)


class PendingTimeoutError(Exception):
    """POST-back tidak tiba dalam batas; fail-closed tanpa silent hang (§5).

    `str(err)` berprefix `permission:` — mirror pola fail-closed
    `gate.PermissionDeniedError` yang dikonsumsi hilir sebagai error
    envelope, bukan degradasi senyap.
    """


class _Pending:
    """Satu server-initiated request yang menunggu POST-back.

    `result`/`resolved` dimutasi hanya di bawah lock ControlChannel;
    `event` menandakan delivery sehingga await thread TIDAK memegang lock
    saat menunggu (transport thread-per-connection tidak boleh saling blok).
    """

    __slots__ = ("client_id", "deadline", "result", "resolved", "event")

    def __init__(self, client_id: str, deadline: float) -> None:
        self.client_id = client_id
        self.deadline = deadline
        self.result: Optional[Dict[str, Any]] = None
        self.resolved = False
        self.event = threading.Event()


def _validate_envelope(envelope: Any) -> None:
    """Envelope outgoing WAJIB shape golden `*_event` — persis lima kunci.

    Field ekstra DITOLAK (bukan di-drop): envelope keluar ke wire, dan
    kesalahan produsen (U31/U23) harus terdengar keras, bukan disaring
    senyap yang menyamarkan bug shape (WIRE_PROTOCOL §7).
    """
    if not isinstance(envelope, dict):
        raise ValueError(
            f"control: envelope must be an object (golden `*_event`), got {type(envelope).__name__}"
        )
    if set(envelope) != ENVELOPE_KEYS:
        raise ValueError(
            f"control: envelope keys {sorted(envelope)} must equal golden keys "
            f"{sorted(ENVELOPE_KEYS)} (no extra fields; no missing fields)"
        )
    if not isinstance(envelope["type"], str):
        raise ValueError("control: envelope 'type' must be a string")


def _encode_event(envelope: Dict[str, Any]) -> bytes:
    """Frame SSE `data: {json}\\n\\n`.

    JSON UTF-8 verbatim (mirror serde_json / JSON.stringify). Control chars
    di-escape oleh json.dumps sehingga payload ber-newline tidak memecah
    framing `data:`.
    """
    data = json.dumps(envelope, ensure_ascii=False, separators=(",", ":"))
    return f"data: {data}\n\n".encode("utf-8")


class ControlChannel:
    """Control channel thread-safe untuk ThreadingHTTPServer (transport U31).

    Satu ControlChannel per server: `_clients` memetakan client_id ke writer
    SSE, `_pending` memetakan correlation_id ke request yang menunggu
    POST-back. Semua mutasi state di bawah satu lock; keepalive + sweep TTL
    berjalan di satu thread daemon.
    """

    def __init__(self, keepalive_interval: float = DEFAULT_KEEPALIVE_INTERVAL) -> None:
        if not isinstance(keepalive_interval, (int, float)) or keepalive_interval <= 0:
            raise ValueError("control: keepalive_interval must be a positive number")
        self._keepalive_interval = float(keepalive_interval)
        self._lock = threading.RLock()
        self._clients: Dict[str, Callable[[bytes], None]] = {}
        self._pending: Dict[str, _Pending] = {}
        self._stop = threading.Event()
        self._keepalive_thread = threading.Thread(
            target=self._keepalive_loop,
            name=f"control-keepalive-{next(_keepalive_seq)}",
            daemon=True,
        )
        self._keepalive_thread.start()


    def register_client(self, client_id: str, writer: Callable[[bytes], None]) -> None:
        """Daftarkan (atau ganti, pada reconnect) writer SSE client.

        Satu client_id satu koneksi aktif: re-register MENGgantikan writer
        lama (mirror Rust `insert`). Writer adalah callable `(bytes) -> None`
        milik transport; kegagalannya dianggap koneksi mati.
        """
        if not isinstance(client_id, str) or not client_id:
            raise ValueError("control: client_id must be a non-empty string")
        if not callable(writer):
            raise TypeError("control: writer must be callable (bytes) -> None")
        with self._lock:
            self._clients[client_id] = writer

    def unregister_client(
        self, client_id: str, writer: Optional[Callable[[bytes], None]] = None
    ) -> bool:
        """Hentikan keepalive client dan hapus presence-nya (idempotent).

        True bila ada client yang dihapus; False bila tidak terdaftar.
        """
        with self._lock:
            if writer is not None and self._clients.get(client_id) is not writer:
                return False
            removed = self._clients.pop(client_id, None)
        if removed is not None:
            logger.debug("control: unregistered client '%s'", client_id)
        return removed is not None

    def is_client_connected(self, client_id: str) -> bool:
        """True bila client terdaftar dengan stream SSE aktif (presence).

        Dipakai fail-closed routing remote (U23): request ke client yang
        tidak terhubung ditolak SEBELUM push envelope.
        """
        with self._lock:
            return client_id in self._clients


    def push(self, client_id: str, envelope: Dict[str, Any]) -> bool:
        """Kirim satu envelope event ke client; frame `data: {...}\\n\\n`.

        Envelope WAJIB shape golden `*_event` (lima kunci persis — nol field
        ekstra). True bila frame terkirim; False bila client tidak terdaftar
        atau writer gagal (mirror Rust `push -> bool`); writer yang gagal
        membuat client di-unregister (fail-closed presence).
        """
        _validate_envelope(envelope)
        with self._lock:
            writer = self._clients.get(client_id)
        if writer is None:
            logger.debug("control: push to unregistered client '%s' dropped", client_id)
            return False
        frame = _encode_event(envelope)
        try:
            writer(frame)
        except Exception:
            logger.debug("control: writer for client '%s' failed; unregistering", client_id)
            self.unregister_client(client_id, writer)
            return False
        return True


    def create_correlation(self, client_id: str, ttl_secs: float = DEFAULT_CORRELATION_TTL) -> str:
        """Buat correlation_id unik dan catat pending request dengan deadline.

        TTL default 60s (WIRE_PROTOCOL §5). Pending dihapus saat POST-back
        tiba, saat await timeout/cancel, atau disweep oleh loop keepalive
        (hygiene memori untuk entri yang ditinggalkan).
        """
        if not isinstance(client_id, str) or not client_id:
            raise ValueError("control: client_id must be a non-empty string")
        if not isinstance(ttl_secs, (int, float)) or ttl_secs <= 0:
            raise ValueError("control: ttl_secs must be a positive number")
        correlation_id = str(uuid.uuid4())
        with self._lock:
            self._pending[correlation_id] = _Pending(
                client_id=client_id, deadline=time.monotonic() + float(ttl_secs)
            )
        return correlation_id

    def cancel_pending(self, correlation_id: str) -> bool:
        """Hapus pending tanpa menunggu POST-back (path fail-closed caller:
        client tidak terhubung, error sebelum push, dsb). Mirror Rust
        `cancel_pending`. True bila entri ada dan dihapus."""
        with self._lock:
            removed = self._pending.pop(correlation_id, None)
        return removed is not None

    def resolve_postback(self, correlation_id: str, body: Dict[str, Any]) -> bool:
        """Cocokkan POST-back ke pending request (shape golden `postback_*`).

        Body dinormalisasi via `wire.build_postback` — hasil PERSIS empat
        kunci golden `{correlation_id, ok, payload, error}`. POST-back yang
        TIDAK dikenal (unknown / expired / duplikat / mismatch path) diabaikan
        dan di-log WARNING (bukan error) — mirror Rust `complete_postback ->
        false`; expired fail-closed: waiter sudah/bakal gagal sendiri.
        """
        if not isinstance(body, dict):
            raise ValueError(
                f"control: POST-back body must be an object, got {type(body).__name__}"
            )
        normalized = build_postback(body)
        if normalized["correlation_id"] != correlation_id:
            logger.warning(
                "control: POST-back correlation '%s' does not match path '%s'; ignoring",
                normalized["correlation_id"],
                correlation_id,
            )
            return False
        with self._lock:
            pending = self._pending.get(correlation_id)
            if pending is None:
                logger.warning(
                    "control: ignoring POST-back for unknown correlation '%s'",
                    correlation_id,
                )
                return False
            if pending.resolved:
                logger.warning(
                    "control: ignoring duplicate POST-back for correlation '%s'",
                    correlation_id,
                )
                return False
            if time.monotonic() > pending.deadline:
                self._pending.pop(correlation_id, None)
                logger.warning(
                    "control: ignoring POST-back for expired correlation '%s'",
                    correlation_id,
                )
                return False
            pending.result = normalized
            pending.resolved = True
            pending.event.set()
            return True

    def await_postback(self, correlation_id: str, timeout_secs: float) -> Dict[str, Any]:
        """Blok sampai POST-back tiba atau batas tercapai; timeout fail-closed.

        Batas tunggu = min(timeout_secs, sisa TTL): correlation yang
        deadline-nya lewat TIDAK pernah diterima (TTL expire = fail-closed,
        WIRE_PROTOCOL §5). Timeout menimbulkan `PendingTimeoutError`
        berprefix `permission:` — error eksplisit di sisi core, bukan
        silent hang. Mengembalikan body POST-back hasil normalisasi.


        Entri dikonsumsi di kedua jalur sukses (fast-path di atas dan setelah event) — satu
        correlation satu delivery; sweep tidak akan menyingkirkan entri yang masih valid di
        antara resolve dan pop ini karena wait_secs dibatasi oleh sisa TTL.
        """
        if not isinstance(timeout_secs, (int, float)) or timeout_secs < 0:
            raise ValueError("control: timeout_secs must be a non-negative number")
        with self._lock:
            pending = self._pending.get(correlation_id)
            if pending is None:
                raise ValueError(
                    f"control: await_postback on unknown or expired correlation "
                    f"'{correlation_id}' (no pending request)"
                )
            if pending.resolved:
                self._pending.pop(correlation_id, None)
                if pending.result is None:
                    raise ValueError(f"control: resolved pending '{correlation_id}' has no result")
                return pending.result
            remaining = pending.deadline - time.monotonic()
        wait_secs = min(timeout_secs, max(remaining, 0.0))
        if wait_secs <= 0:
            with self._lock:
                self._pending.pop(correlation_id, None)
            raise PendingTimeoutError(
                f"permission: correlation '{correlation_id}' timed out waiting for POST-back"
            )
        if not pending.event.wait(wait_secs):
            with self._lock:
                self._pending.pop(correlation_id, None)
            raise PendingTimeoutError(
                f"permission: correlation '{correlation_id}' timed out waiting for POST-back"
            )
        with self._lock:
            self._pending.pop(correlation_id, None)
        if pending.result is None:
            raise ValueError(f"control: pending '{correlation_id}' resolved with no result")
        return pending.result

    def pending_len(self) -> int:
        """Jumlah pending request (debug/test aid; mirror Rust)."""
        with self._lock:
            return len(self._pending)


    def _keepalive_loop(self) -> None:
        while not self._stop.wait(self._keepalive_interval):
            try:
                self._send_keepalives()
                self._sweep_expired()
            except Exception:
                logger.exception("control: keepalive cycle failed")

    def _send_keepalives(self) -> None:
        with self._lock:
            clients = list(self._clients.items())
        for client_id, writer in clients:
            try:
                writer(KEEPALIVE_FRAME)
            except Exception:
                logger.debug("control: keepalive failed for client '%s'; unregistering", client_id)
                self.unregister_client(client_id, writer)

    def _sweep_expired(self) -> None:
        now = time.monotonic()
        with self._lock:
            for correlation_id, pending in list(self._pending.items()):
                if now > pending.deadline:
                    del self._pending[correlation_id]

    def close(self) -> None:
        """Hentikan thread keepalive + sweep (idempotent).

        Dipanggil lifecycle facade (U32 `AgentServer.stop`) agar thread
        daemon tidak hidup lebih lama dari server yang memilikinya. Setelah
        close, channel tidak bisa dipakai lagi untuk keepalive (re-start
        adalah kesalahan program; facade mencegahnya di tingkat server).
        """
        self._stop.set()
        if self._keepalive_thread.is_alive():
            self._keepalive_thread.join(timeout=5.0)
