//! Per-client rate limit of the public forms (AGENTS.md §7.2).
//!
//! A client is known only by its fingerprint `HMAC-SHA-256(daily salt, IP)`. The salt is random,
//! kept **in memory only** (never stored, logged nor backed up), replaced every 24 hours, and
//! the counters are destroyed with it: no counter outlives 24 hours. IPv6 clients are grouped by
//! /64 prefix, the block a single subscriber usually gets. With several api replicas the limit
//! applies per replica (accepted by §7.2).
//!
//! Windows are fixed: the hour (or day) starts with the first counted request of the client.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::config::AntispamConfig;
use crate::http::error::ApiError;

const HOUR: Duration = Duration::from_secs(3_600);
const DAY: Duration = Duration::from_secs(86_400);
/// Lifetime of a salt, and therefore of every counter.
pub const SALT_TTL: Duration = DAY;
/// Period of the removal of expired counters.
const PURGE_INTERVAL: Duration = Duration::from_secs(600);
/// Tracked clients beyond which new clients are refused for a minute (memory bound under a
/// flood of distinct addresses).
const MAX_CLIENTS: usize = 200_000;

/// Rate-limited form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RateAction {
    Idea,
    Contribution,
    Report,
}

/// Limits per fingerprint (`RATE_LIMIT_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimits {
    pub ideas_per_hour: u32,
    pub ideas_per_day: u32,
    pub contributions_per_day: u32,
    pub reports_per_day: u32,
}

impl From<&AntispamConfig> for RateLimits {
    fn from(config: &AntispamConfig) -> Self {
        Self {
            ideas_per_hour: config.ideas_per_hour,
            ideas_per_day: config.ideas_per_day,
            contributions_per_day: config.contributions_per_day,
            reports_per_day: config.reports_per_day,
        }
    }
}

impl RateLimits {
    /// `(hourly limit, daily limit)` of an action.
    fn of(&self, action: RateAction) -> (Option<u32>, u32) {
        match action {
            RateAction::Idea => (Some(self.ideas_per_hour), self.ideas_per_day),
            RateAction::Contribution => (None, self.contributions_per_day),
            RateAction::Report => (None, self.reports_per_day),
        }
    }
}

/// Which window refused the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatePeriod {
    Hour,
    Day,
}

/// Refused request: limit reached until `retry_after`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimited {
    pub action: RateAction,
    /// `None` when the server tracks too many clients.
    pub period: Option<RatePeriod>,
    pub limit: u32,
    pub retry_after: Duration,
}

/// « 23 minutes », « 1 minute », « 5 heures ».
fn french_delay(delay: Duration) -> String {
    let minutes = delay.as_secs().div_ceil(60).max(1);
    if minutes < 60 {
        format!("{minutes} minute{}", if minutes > 1 { "s" } else { "" })
    } else {
        let hours = minutes.div_ceil(60);
        format!("{hours} heure{}", if hours > 1 { "s" } else { "" })
    }
}

impl RateLimited {
    /// French message shown to the visitor.
    pub fn message(&self) -> String {
        let wait = french_delay(self.retry_after);
        let Some(period) = self.period else {
            return format!("Service très sollicité : veuillez réessayer dans {wait}.");
        };
        let plural = if self.limit > 1 { "s" } else { "" };
        let what = match self.action {
            RateAction::Idea => format!("idée{plural}"),
            RateAction::Contribution => format!("contribution{plural}"),
            RateAction::Report => format!("signalement{plural}"),
        };
        let per = match period {
            RatePeriod::Hour => "par heure",
            RatePeriod::Day => "par jour",
        };
        format!(
            "Vous avez atteint la limite de {} {what} {per} depuis votre connexion. \
             Veuillez réessayer dans {wait}.",
            self.limit
        )
    }
}

impl From<RateLimited> for ApiError {
    fn from(limited: RateLimited) -> Self {
        ApiError::new(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            limited.message(),
        )
        .with_retry_after(limited.retry_after.as_secs().max(1))
    }
}

#[derive(Debug, Clone, Copy)]
struct Window {
    start: Instant,
    count: u32,
}

impl Window {
    fn new(now: Instant) -> Self {
        Self {
            start: now,
            count: 0,
        }
    }

    /// Restarts the window once its period is over.
    fn refresh(&mut self, now: Instant, period: Duration) {
        if now.duration_since(self.start) >= period {
            *self = Self::new(now);
        }
    }

    fn remaining(&self, now: Instant, period: Duration) -> Duration {
        (self.start + period).saturating_duration_since(now)
    }
}

#[derive(Debug, Clone, Copy)]
struct Counter {
    hour: Window,
    day: Window,
}

impl Counter {
    fn expired(&self, now: Instant) -> bool {
        now.duration_since(self.day.start) >= DAY && now.duration_since(self.hour.start) >= HOUR
    }
}

struct State {
    salt: [u8; 32],
    salt_created: Instant,
    counters: HashMap<([u8; 32], RateAction), Counter>,
    last_purge: Instant,
}

impl State {
    fn new(now: Instant) -> Self {
        Self {
            salt: new_salt(),
            salt_created: now,
            counters: HashMap::new(),
            last_purge: now,
        }
    }
}

fn new_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    rand::fill(&mut salt[..]);
    salt
}

/// In-memory rate limiter.
pub struct RateLimiter {
    limits: RateLimits,
    state: Mutex<State>,
}

impl std::fmt::Debug for RateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimiter")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

/// Bytes identifying a client: the IPv4 address, or the /64 prefix of an IPv6 address.
fn client_key(ip: IpAddr) -> Vec<u8> {
    match ip {
        IpAddr::V4(v4) => [&[4u8][..], &v4.octets()].concat(),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => [&[4u8][..], &v4.octets()].concat(),
            None => [&[6u8][..], &v6.octets()[..8]].concat(),
        },
    }
}

fn fingerprint(salt: &[u8; 32], ip: IpAddr) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(salt).expect("HMAC accepts any key");
    mac.update(&client_key(ip));
    mac.finalize().into_bytes().into()
}

impl RateLimiter {
    pub fn new(limits: RateLimits) -> Self {
        Self {
            limits,
            state: Mutex::new(State::new(Instant::now())),
        }
    }

    /// Counts a request of `ip` for `action`, or refuses it when a limit is reached (a refused
    /// request is not counted).
    pub fn check(&self, ip: IpAddr, action: RateAction) -> Result<(), RateLimited> {
        self.check_at(ip, action, Instant::now())
    }

    fn check_at(&self, ip: IpAddr, action: RateAction, now: Instant) -> Result<(), RateLimited> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if now.duration_since(state.salt_created) >= SALT_TTL {
            // The old salt and every fingerprint computed with it are destroyed.
            *state = State::new(now);
        }
        if now.duration_since(state.last_purge) >= PURGE_INTERVAL
            || state.counters.len() >= MAX_CLIENTS
        {
            state.counters.retain(|_, counter| !counter.expired(now));
            state.last_purge = now;
        }
        let until_new_salt = (state.salt_created + SALT_TTL).saturating_duration_since(now);
        let key = (fingerprint(&state.salt, ip), action);
        if !state.counters.contains_key(&key) && state.counters.len() >= MAX_CLIENTS {
            tracing::warn!("rate limiter full: new clients refused for a minute");
            return Err(RateLimited {
                action,
                period: None,
                limit: 0,
                retry_after: Duration::from_secs(60),
            });
        }
        let counter = state.counters.entry(key).or_insert(Counter {
            hour: Window::new(now),
            day: Window::new(now),
        });
        counter.hour.refresh(now, HOUR);
        counter.day.refresh(now, DAY);

        let (hourly, daily) = self.limits.of(action);
        let refused = if counter.day.count >= daily {
            Some((RatePeriod::Day, daily, counter.day.remaining(now, DAY)))
        } else {
            hourly
                .filter(|limit| counter.hour.count >= *limit)
                .map(|limit| (RatePeriod::Hour, limit, counter.hour.remaining(now, HOUR)))
        };
        if let Some((period, limit, remaining)) = refused {
            return Err(RateLimited {
                action,
                period: Some(period),
                limit,
                retry_after: remaining.min(until_new_salt),
            });
        }
        counter.hour.count += 1;
        counter.day.count += 1;
        Ok(())
    }

    #[cfg(test)]
    fn tracked(&self) -> usize {
        self.state.lock().unwrap().counters.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: RateLimits = RateLimits {
        ideas_per_hour: 2,
        ideas_per_day: 3,
        contributions_per_day: 2,
        reports_per_day: 1,
    };

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn limiter() -> (RateLimiter, Instant) {
        let limiter = RateLimiter::new(LIMITS);
        let start = limiter.state.lock().unwrap().salt_created;
        (limiter, start)
    }

    #[test]
    fn hourly_and_daily_idea_limits() {
        let (limiter, t0) = limiter();
        let client = ip("203.0.113.7");
        limiter.check_at(client, RateAction::Idea, t0).unwrap();
        limiter
            .check_at(client, RateAction::Idea, t0 + Duration::from_secs(60))
            .unwrap();
        let refused = limiter
            .check_at(client, RateAction::Idea, t0 + Duration::from_secs(600))
            .unwrap_err();
        assert_eq!(refused.period, Some(RatePeriod::Hour));
        assert_eq!(refused.limit, 2);
        assert_eq!(refused.retry_after, Duration::from_secs(3_000));
        // Refused requests are not counted: the next hour accepts one more, then the day is
        // full.
        limiter
            .check_at(client, RateAction::Idea, t0 + HOUR)
            .unwrap();
        let refused = limiter
            .check_at(client, RateAction::Idea, t0 + HOUR + Duration::from_secs(1))
            .unwrap_err();
        assert_eq!(refused.period, Some(RatePeriod::Day));
        assert_eq!(refused.limit, 3);
        assert_eq!(refused.retry_after, DAY - HOUR - Duration::from_secs(1));
    }

    #[test]
    fn actions_and_clients_are_counted_separately() {
        let (limiter, t0) = limiter();
        let a = ip("203.0.113.7");
        let b = ip("198.51.100.1");
        limiter.check_at(a, RateAction::Report, t0).unwrap();
        assert!(limiter.check_at(a, RateAction::Report, t0).is_err());
        limiter.check_at(b, RateAction::Report, t0).unwrap();
        limiter.check_at(a, RateAction::Contribution, t0).unwrap();
        limiter.check_at(a, RateAction::Contribution, t0).unwrap();
        let refused = limiter
            .check_at(a, RateAction::Contribution, t0)
            .unwrap_err();
        assert_eq!(refused.period, Some(RatePeriod::Day));
        limiter.check_at(a, RateAction::Idea, t0).unwrap();
    }

    #[test]
    fn ipv6_clients_are_grouped_by_64_prefix() {
        let (limiter, t0) = limiter();
        limiter
            .check_at(ip("2001:db8:1:2::1"), RateAction::Report, t0)
            .unwrap();
        assert!(
            limiter
                .check_at(ip("2001:db8:1:2:ffff::9"), RateAction::Report, t0)
                .is_err()
        );
        limiter
            .check_at(ip("2001:db8:1:3::1"), RateAction::Report, t0)
            .unwrap();
        // An IPv4-mapped address is the IPv4 client.
        limiter
            .check_at(ip("192.0.2.1"), RateAction::Report, t0)
            .unwrap();
        assert!(
            limiter
                .check_at(ip("::ffff:192.0.2.1"), RateAction::Report, t0)
                .is_err()
        );
    }

    #[test]
    fn the_daily_salt_and_its_counters_are_destroyed() {
        let (limiter, t0) = limiter();
        let client = ip("203.0.113.7");
        let salt = limiter.state.lock().unwrap().salt;
        let before = fingerprint(&salt, client);
        limiter.check_at(client, RateAction::Report, t0).unwrap();
        let refused = limiter
            .check_at(client, RateAction::Report, t0 + DAY - HOUR)
            .unwrap_err();
        // The retry never goes beyond the end of the salt.
        assert_eq!(refused.retry_after, HOUR);
        limiter
            .check_at(client, RateAction::Report, t0 + DAY)
            .unwrap();
        let new_salt = limiter.state.lock().unwrap().salt;
        assert_ne!(new_salt, salt);
        assert_ne!(fingerprint(&new_salt, client), before);
        assert_eq!(limiter.tracked(), 1);
    }

    #[test]
    fn expired_counters_are_purged() {
        let (limiter, t0) = limiter();
        for i in 0..10u8 {
            limiter
                .check_at(IpAddr::from([10, 0, 0, i]), RateAction::Report, t0)
                .unwrap();
        }
        assert_eq!(limiter.tracked(), 10);
        // Hour windows are over but day windows are not: kept.
        limiter
            .check_at(ip("10.0.1.1"), RateAction::Report, t0 + 2 * HOUR)
            .unwrap();
        assert_eq!(limiter.tracked(), 11);
    }

    #[test]
    fn fingerprints_are_keyed_and_stable() {
        let salt = [7u8; 32];
        assert_eq!(
            fingerprint(&salt, ip("203.0.113.7")),
            fingerprint(&salt, ip("203.0.113.7"))
        );
        assert_ne!(
            fingerprint(&salt, ip("203.0.113.7")),
            fingerprint(&[8u8; 32], ip("203.0.113.7"))
        );
        assert_ne!(client_key(ip("1.2.3.4")), client_key(ip("::102:304")));
    }

    #[test]
    fn refusals_become_429_with_french_messages() {
        let refused = RateLimited {
            action: RateAction::Idea,
            period: Some(RatePeriod::Hour),
            limit: 5,
            retry_after: Duration::from_secs(1_381),
        };
        let err = ApiError::from(refused);
        assert_eq!(err.status, axum::http::StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(err.code, "rate_limited");
        assert_eq!(err.retry_after, Some(1_381));
        assert_eq!(
            err.message,
            "Vous avez atteint la limite de 5 idées par heure depuis votre connexion. \
             Veuillez réessayer dans 24 minutes."
        );
        let refused = RateLimited {
            action: RateAction::Report,
            period: Some(RatePeriod::Day),
            limit: 20,
            retry_after: Duration::from_secs(5 * 3_600 + 1),
        };
        assert!(
            refused
                .message()
                .contains("limite de 20 signalements par jour")
        );
        let one = RateLimited {
            limit: 1,
            ..refused
        };
        assert!(one.message().contains("limite de 1 signalement par jour"));
        assert!(refused.message().ends_with("dans 6 heures."));
        assert_eq!(french_delay(Duration::from_secs(1)), "1 minute");
        assert_eq!(french_delay(Duration::from_secs(3_600)), "1 heure");
        let full = RateLimited {
            action: RateAction::Contribution,
            period: None,
            limit: 0,
            retry_after: Duration::from_secs(60),
        };
        assert_eq!(
            full.message(),
            "Service très sollicité : veuillez réessayer dans 1 minute."
        );
    }

    #[test]
    fn debug_output_hides_the_salt() {
        let (limiter, t0) = limiter();
        limiter
            .check_at(ip("203.0.113.7"), RateAction::Idea, t0)
            .unwrap();
        let debug = format!("{limiter:?}");
        assert!(!debug.contains("salt"));
        assert!(!debug.contains("203.0.113.7"));
    }
}
