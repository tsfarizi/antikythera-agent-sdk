"""Default-deny permission gate for tool execution and hooks (unit U13, E13).

Sumber kebenaran perilaku:
- npm/antikythera-sdk/runtime/policy.js — JS gate: deny default, boolean
  `allows`, pesan `permission: tool 'X' not in allowlist`.
- antikythera-server-runtime/src/config.rs — Rust GatePolicy: allowlist per
  destination server/client/mcp, deny-all posture, pesan
  `permission: hook 'X' not allowed`.
- contracts/shared/wire_protocol.golden.json — `postback_gate_denial` dan
  `error_event` memakai pesan `permission: tool 'rm' not in allowlist`.

Invariant R4: setiap denial WAJIB menjadi error berprefix `permission:`;
tidak ada degradasi senyap. Tool TIDAK boleh dieksekusi kecuali di-allowlist
destination-nya; hook TIDAK boleh dipanggil kecuali di-allow. Denial dideteksi
secara eksplisit lewat `PermissionDeniedError` (transport HTTP 403, postback
`ok=false`), sementara `check_*` menahan eksekusi di titik panggil.
"""

from __future__ import annotations

import threading
from typing import Dict, FrozenSet, Optional, Set

#: Destination yang dikontrak E13 — nilai identik dengan wire.WIRE OWNER_*;
#: gate berdiri sendiri dan tidak bergantung pada wire.py.
DESTINATIONS: FrozenSet[str] = frozenset({"server", "client", "mcp"})

#: Prefix denial persis (lowercase + titik dua) — invariant R4.
PERMISSION_PREFIX = "permission:"


class PermissionDeniedError(Exception):
    """Denial gate. `str(err)` (dan `.message`) berprefix `permission:`.

    Exception khusus adalah mekanisme deteksi denial yang dikonsumsi unit
    hilir: transport memetakannya ke HTTP 403 `{"error": ...}`, control
    memetakannya ke POST-back `ok=false`, loop_owner membiarkannya meluas
    sebagai error loop.
    """

    @property
    def message(self) -> str:
        return str(self)


def _denied(reason: str) -> PermissionDeniedError:
    """Bangun denial dengan prefix invariant R4 di satu titik — mencegah
    satu instance denial lolos tanpa prefix."""
    return PermissionDeniedError(f"{PERMISSION_PREFIX} {reason}")


class PolicyGate:
    """Allowlist tool per destination dan allowlist hook; default deny (R4).


    -- snapshot allowlist (immutable; tidak membocorkan state internal) -.
    """

    def __init__(self, llm_quota: Optional[int] = None) -> None:
        self._tools: Dict[str, Set[str]] = {d: set() for d in DESTINATIONS}
        self._hooks: Set[str] = set()
        self._llm_lock = threading.Lock()
        self._llm_calls: Dict[str, int] = {}
        self.set_llm_quota(llm_quota)

    def set_llm_quota(self, quota: Optional[int]) -> None:
        if quota is not None and (
            not isinstance(quota, int) or isinstance(quota, bool) or quota < 0
        ):
            raise ValueError("llm quota must be a non-negative integer or None")
        with self._llm_lock:
            self.llm_quota = quota

    def check_llm(self, session_id: Optional[str] = None) -> None:
        with self._llm_lock:
            if self.llm_quota is None:
                return
            key = session_id or "default"
            if key not in self._llm_calls and len(self._llm_calls) >= 1024:
                raise _denied("llm session capacity exceeded")
            used = self._llm_calls.get(key, 0)
            if used >= self.llm_quota:
                raise _denied("llm quota exceeded")
            self._llm_calls[key] = used + 1


    def allow_tool(self, destination: str, name: str) -> None:
        """Izinkan tool `name` untuk `destination` saja; destination lain
        tidak terpengaruh (isolasi per destination)."""
        self._check_destination(destination)
        if not isinstance(name, str):
            raise TypeError(f"tool name must be a string, got {type(name).__name__}")
        self._tools[destination].add(name)

    def allow_hook(self, name: str) -> None:
        """Izinkan hook `name` memanggil peer."""
        if not isinstance(name, str):
            raise TypeError(f"hook name must be a string, got {type(name).__name__}")
        self._hooks.add(name)


    def check_tool(self, destination: str, name: str) -> None:
        """Gagal dengan `PermissionDeniedError` bila tool tidak di-allowlist.

        Lolos (return None) berarti eksekusi DIIZINKAN; pemanggil hanya
        boleh mengeksekusi tool SETELAH check ini lulus.
        """
        self._check_destination(destination)
        if name not in self._tools[destination]:
            raise _denied(f"tool '{name}' not in allowlist")

    def check_hook(self, name: str) -> None:
        """Gagal dengan `PermissionDeniedError` bila hook tidak di-allow."""
        if name not in self._hooks:
            raise _denied(f"hook '{name}' not allowed")


    def allows(self, destination: str, name: str) -> bool:
        """True bila tool di-allowlist destination (mirror policy.js `allows`)."""
        self._check_destination(destination)
        return name in self._tools[destination]

    def allows_hook(self, name: str) -> bool:
        """True bila hook di-allow."""
        return name in self._hooks


    def allowed_tools(self, destination: str) -> FrozenSet[str]:
        """Snapshot immutable allowlist tool `destination`."""
        self._check_destination(destination)
        return frozenset(self._tools[destination])

    def allowed_hooks(self) -> FrozenSet[str]:
        """Snapshot immutable allowlist hook."""
        return frozenset(self._hooks)


    def _check_destination(self, destination: str) -> None:
        """Destination tak dikenal adalah program error, BUKAN denial:
        menolak diam-diam di destination invalid akan menyamarkan bug
        menjadi `permission:` dan melanggar invariant R4 (prefix hanya
        untuk denial nyata)."""
        if destination not in DESTINATIONS:
            raise ValueError(
                f"unknown destination {destination!r}; expected one of {sorted(DESTINATIONS)}"
            )
