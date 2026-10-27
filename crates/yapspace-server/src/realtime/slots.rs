//! Connection caps (docs/architecture/realtime.md §G.3): per IP, per authenticated user, and in total. A slot is
//! released when its guard drops, so a connection always frees its capacity, even after an error.

use std::{
    net::IpAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use dashmap::DashMap;
use uuid::Uuid;

/// Sockets allowed from one client address. IPv6 is counted per /64, see client_ip.rs.
pub const MAX_PER_IP: usize = 20;
/// Sockets one authenticated account may hold.
pub const MAX_PER_USER: usize = 10;

/// Why a socket was refused before the upgrade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Too many sockets from this address (HTTP 429).
    PerIp,
    /// The server is full (HTTP 503).
    Total,
}

#[derive(Default)]
pub struct WsCounters {
    per_ip: DashMap<IpAddr, usize>,
    per_user: DashMap<Uuid, usize>,
    total: AtomicUsize,
    max_total: usize,
}

impl WsCounters {
    pub fn new(max_total: usize) -> Self {
        Self {
            max_total,
            ..Self::default()
        }
    }

    /// Reserves a slot for a new socket from `ip`.
    pub fn acquire_ip(self: &Arc<Self>, ip: IpAddr) -> Result<IpSlot, Refused> {
        if self.total.load(Ordering::Acquire) >= self.max_total {
            return Err(Refused::Total);
        }
        {
            let mut count = self.per_ip.entry(ip).or_insert(0);
            if *count >= MAX_PER_IP {
                return Err(Refused::PerIp);
            }
            *count += 1;
        }
        self.total.fetch_add(1, Ordering::AcqRel);
        Ok(IpSlot {
            counters: Arc::clone(self),
            ip,
        })
    }

    /// Reserves a slot for an authenticated account. `None` means the account is at its limit.
    pub fn acquire_user(self: &Arc<Self>, user: Uuid) -> Option<UserSlot> {
        let mut count = self.per_user.entry(user).or_insert(0);
        if *count >= MAX_PER_USER {
            return None;
        }
        *count += 1;
        Some(UserSlot {
            counters: Arc::clone(self),
            user,
        })
    }

    /// Sockets currently open, across all clients.
    pub fn total(&self) -> usize {
        self.total.load(Ordering::Acquire)
    }
}

/// Released when the socket it was taken for closes.
pub struct IpSlot {
    counters: Arc<WsCounters>,
    ip: IpAddr,
}

impl Drop for IpSlot {
    fn drop(&mut self) {
        self.counters.total.fetch_sub(1, Ordering::AcqRel);
        self.counters.per_ip.remove_if_mut(&self.ip, |_, count| {
            *count = count.saturating_sub(1);
            *count == 0
        });
    }
}

/// Held by a socket after it has authenticated.
pub struct UserSlot {
    counters: Arc<WsCounters>,
    user: Uuid,
}

impl Drop for UserSlot {
    fn drop(&mut self) {
        self.counters.per_user.remove_if_mut(&self.user, |_, count| {
            *count = count.saturating_sub(1);
            *count == 0
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_ip_limit_and_release() {
        let counters = Arc::new(WsCounters::new(1000));
        let ip: IpAddr = "192.0.2.1".parse().unwrap();
        let mut held: Vec<IpSlot> = (0..MAX_PER_IP)
            .map(|_| counters.acquire_ip(ip).unwrap())
            .collect();
        assert_eq!(counters.acquire_ip(ip).err(), Some(Refused::PerIp));
        held.pop();
        assert!(counters.acquire_ip(ip).is_ok(), "a released slot is reusable");
    }

    #[test]
    fn total_limit_refuses_with_total() {
        let counters = Arc::new(WsCounters::new(2));
        let a = counters.acquire_ip("192.0.2.1".parse().unwrap()).unwrap();
        let _b = counters.acquire_ip("192.0.2.2".parse().unwrap()).unwrap();
        assert_eq!(
            counters.acquire_ip("192.0.2.3".parse().unwrap()).err(),
            Some(Refused::Total)
        );
        drop(a);
        assert_eq!(counters.total(), 1);
    }

    #[test]
    fn per_user_limit_counts_only_authenticated_sockets() {
        let counters = Arc::new(WsCounters::new(1000));
        let user = Uuid::now_v7();
        let slots: Vec<UserSlot> = (0..MAX_PER_USER)
            .map(|_| counters.acquire_user(user).unwrap())
            .collect();
        assert!(counters.acquire_user(user).is_none());
        drop(slots);
        assert!(counters.acquire_user(user).is_some());
    }
}
