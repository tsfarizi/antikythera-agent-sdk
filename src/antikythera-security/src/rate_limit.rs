//! Rate limiting with sliding windows per minute/hour/day and session tracking.

use antikythera_domain::security::RateLimitConfig;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Rate limit error.
#[derive(Debug, Clone, Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded: {current}/{limit} requests per {window_secs}s")]
    LimitExceeded {
        limit: u32,
        current: u32,
        window_secs: u64,
    },

    #[error("Too many concurrent sessions: {current}/{max}")]
    TooManyConcurrentSessions { max: u32, current: u32 },
}

#[derive(Debug, Clone)]
struct TimeWindow {
    requests: Vec<Instant>,
    window_size: Duration,
    max_requests: u32,
    burst_allowance: u32,
}

impl TimeWindow {
    fn new(window_size_secs: u32, max_requests: u32, burst_allowance: u32) -> Self {
        Self {
            requests: Vec::new(),
            window_size: Duration::from_secs(window_size_secs as u64),
            max_requests,
            burst_allowance,
        }
    }

    fn check(&mut self) -> Result<(), RateLimitError> {
        let now = Instant::now();
        self.requests
            .retain(|&ts| now.duration_since(ts) < self.window_size);

        let effective_limit = self.max_requests + self.burst_allowance;
        if self.requests.len() as u32 >= effective_limit {
            return Err(RateLimitError::LimitExceeded {
                limit: self.max_requests,
                current: self.requests.len() as u32,
                window_secs: self.window_size.as_secs(),
            });
        }

        self.requests.push(now);
        Ok(())
    }

    fn reset(&mut self) {
        self.requests.clear();
    }

    fn request_count(&self) -> u32 {
        self.requests.len() as u32
    }
}

#[derive(Debug, Clone)]
struct SessionLimits {
    minute_window: TimeWindow,
    hour_window: TimeWindow,
    day_window: TimeWindow,
    last_activity: Instant,
}

/// Per-session usage statistics.
#[derive(Debug, Clone)]
pub struct SessionUsage {
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub requests_per_day: u32,
    pub last_activity: Instant,
}

/// Configurable rate limiter with sliding-window counters.
pub struct RateLimiter {
    config: RateLimitConfig,
    session_limits: Arc<Mutex<HashMap<String, SessionLimits>>>,
    cleanup_task: Option<std::thread::JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let session_limits = Arc::new(Mutex::new(HashMap::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let cleanup_task = if config.enabled {
            let limits_clone = Arc::clone(&session_limits);
            let shutdown_clone = Arc::clone(&shutdown);
            let cleanup_interval = Duration::from_secs(config.cleanup_interval_secs as u64);

            Some(std::thread::spawn(move || {
                Self::cleanup_loop(limits_clone, cleanup_interval, shutdown_clone);
            }))
        } else {
            None
        };

        Self {
            config,
            session_limits,
            cleanup_task,
            shutdown,
        }
    }

    pub fn from_config() -> Self {
        Self::new(RateLimitConfig::default())
    }

    /// Check whether a request from `session_id` is allowed, consuming one
    /// token from each time window.
    pub fn check(&self, session_id: &str) -> Result<(), RateLimitError> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut limits = self
            .session_limits
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        if limits.len() as u32 >= self.config.max_concurrent_sessions
            && !limits.contains_key(session_id)
        {
            tracing::warn!(
                session_id,
                "Rate limit: too many concurrent sessions ({}/{})",
                limits.len(),
                self.config.max_concurrent_sessions
            );
            return Err(RateLimitError::TooManyConcurrentSessions {
                max: self.config.max_concurrent_sessions,
                current: limits.len() as u32,
            });
        }

        let session = limits
            .entry(session_id.to_string())
            .or_insert_with(|| SessionLimits {
                minute_window: TimeWindow::new(
                    60,
                    self.config.requests_per_minute,
                    self.config.burst_allowance,
                ),
                hour_window: TimeWindow::new(
                    3600,
                    self.config.requests_per_hour,
                    self.config.burst_allowance,
                ),
                day_window: TimeWindow::new(
                    86400,
                    self.config.requests_per_day,
                    self.config.burst_allowance,
                ),
                last_activity: Instant::now(),
            });

        session.last_activity = Instant::now();

        if let Err(e) = session.minute_window.check() {
            tracing::warn!(session_id, error = %e, "Rate limit exceeded (minute)");
            return Err(e);
        }
        if let Err(e) = session.hour_window.check() {
            tracing::warn!(session_id, error = %e, "Rate limit exceeded (hour)");
            return Err(e);
        }
        if let Err(e) = session.day_window.check() {
            tracing::warn!(session_id, error = %e, "Rate limit exceeded (day)");
            return Err(e);
        }

        Ok(())
    }

    /// Get current usage for a session.
    pub fn get_usage(&self, session_id: &str) -> Option<SessionUsage> {
        let limits = self
            .session_limits
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        limits.get(session_id).map(|s| SessionUsage {
            requests_per_minute: s.minute_window.request_count(),
            requests_per_hour: s.hour_window.request_count(),
            requests_per_day: s.day_window.request_count(),
            last_activity: s.last_activity,
        })
    }

    /// Reset all counters for a session.
    pub fn reset_session(&self, session_id: &str) {
        let mut limits = self
            .session_limits
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(s) = limits.get_mut(session_id) {
            s.minute_window.reset();
            s.hour_window.reset();
            s.day_window.reset();
        }
    }

    /// Remove a session entirely.
    pub fn remove_session(&self, session_id: &str) {
        let mut limits = self
            .session_limits
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        limits.remove(session_id);
    }

    /// Number of currently tracked sessions.
    pub fn active_session_count(&self) -> usize {
        let limits = self
            .session_limits
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        limits.len()
    }

    /// Current configuration reference.
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Replace config and restart the cleanup task.
    pub fn update_config(&mut self, config: RateLimitConfig) {
        tracing::info!(enabled = config.enabled, "Rate limiter config updated");
        self.config = config;

        if let Some(handle) = self.cleanup_task.take() {
            self.shutdown.store(true, Ordering::Relaxed);
            let _ = handle.join();
        }

        if self.config.enabled {
            self.shutdown.store(false, Ordering::Relaxed);
            let limits_clone = Arc::clone(&self.session_limits);
            let shutdown_clone = Arc::clone(&self.shutdown);
            let cleanup_interval = Duration::from_secs(self.config.cleanup_interval_secs as u64);

            self.cleanup_task = Some(std::thread::spawn(move || {
                Self::cleanup_loop(limits_clone, cleanup_interval, shutdown_clone);
            }));
        }
    }

    fn cleanup_loop(
        limits: Arc<Mutex<HashMap<String, SessionLimits>>>,
        interval: Duration,
        shutdown: Arc<AtomicBool>,
    ) {
        let tick = Duration::from_secs(1);
        let mut elapsed = Duration::ZERO;
        loop {
            std::thread::sleep(tick);
            elapsed += tick;

            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            if elapsed >= interval {
                elapsed = Duration::ZERO;
                let mut guard = limits.lock().unwrap_or_else(|e| e.into_inner());
                let now = Instant::now();
                let timeout = Duration::from_secs(300);
                guard.retain(|_, s| now.duration_since(s.last_activity) < timeout);
            }
        }
    }
}

impl Drop for RateLimiter {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.cleanup_task.take() {
            let _ = handle.join();
        }
    }
}

#[async_trait::async_trait]
impl antikythera_ports::RateLimiter for RateLimiter {
    async fn check_rate_limit(&self, key: &str) -> Result<(), String> {
        self.check(key).map_err(|e| e.to_string())
    }

    /// Check() already records the request inside each TimeWindow::check(). A dedicated
    /// record-only path is not needed; the port contract is satisfied by calling check which both
    /// validates and records.
    ///
    async fn record_request(&self, key: &str) {
        let _ = self.check(key);
    }
}
