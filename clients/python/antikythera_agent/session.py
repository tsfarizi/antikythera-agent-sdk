"""Session management implementation."""

from __future__ import annotations

import time
from typing import Optional

from antikythera_agent.types import SessionInfo


class SessionManager:
    """Session lifecycle management.

    Automatically manages session creation, TTL, and cleanup.

    Example:
        >>> manager = SessionManager(max_sessions=100)
        >>> session = manager.get_or_create("session-123", "coder")
        >>> print(session.session_id)
    """

    def __init__(
        self,
        max_sessions: int = 100,
        session_ttl_ms: int = 3600000,
    ):
        """Create a new SessionManager.

        Args:
            max_sessions: Maximum concurrent sessions.
            session_ttl_ms: Session time-to-live in milliseconds.
        """
        self._max_sessions = max_sessions
        self._ttl_ms = session_ttl_ms
        self._sessions: dict[str, SessionInfo] = {}

    def get_or_create(
        self, session_id: str, agent_id: str
    ) -> SessionInfo:
        """Get or create a session.

        Args:
            session_id: Session identifier.
            agent_id: Agent identifier.

        Returns:
            Session information.


        Returns existing session if found.
        """
        now = int(time.time() * 1000)

        if session_id in self._sessions:
            session = self._sessions[session_id]
            session.last_activity = now
            return session

        if len(self._sessions) >= self._max_sessions:
            self._evict_expired()
            if len(self._sessions) >= self._max_sessions:
                oldest = min(
                    self._sessions.values(), key=lambda s: s.last_activity
                )
                del self._sessions[oldest.session_id]

        session = SessionInfo(
            session_id=session_id,
            agent_id=agent_id,
            created_at=now,
            last_activity=now,
            message_count=0,
        )
        self._sessions[session_id] = session
        return session

    def get(self, session_id: str) -> Optional[SessionInfo]:
        """Get a session by ID.

        Args:
            session_id: Session identifier.

        Returns:
            Session information or None.
        """
        return self._sessions.get(session_id)

    def list_by_agent(self, agent_id: str) -> list[SessionInfo]:
        """List sessions for an agent.

        Args:
            agent_id: Agent identifier.

        Returns:
            List of sessions.
        """
        return [s for s in self._sessions.values() if s.agent_id == agent_id]

    def list_all(self) -> list[SessionInfo]:
        """List all sessions.

        Returns:
            All sessions.
        """
        return list(self._sessions.values())

    def remove(self, session_id: str) -> Optional[SessionInfo]:
        """Remove a session.

        Args:
            session_id: Session identifier.

        Returns:
            Removed session or None.
        """
        return self._sessions.pop(session_id, None)

    def count(self) -> int:
        """Get session count.

        Returns:
            Number of active sessions.
        """
        return len(self._sessions)

    def _evict_expired(self) -> None:
        """Evict expired sessions."""
        now = int(time.time() * 1000)
        expired = [
            sid
            for sid, session in self._sessions.items()
            if now - session.last_activity > self._ttl_ms
        ]
        for sid in expired:
            del self._sessions[sid]
