//! Manual RSS bench of the upgrade limiter (008 plan-a decision 11). Fills it with N distinct
//! IPv6 `/64` keys (default: the cap) and prints how many it holds. Compare the maximum RSS of
//! `0` and of the default, in release:
//!
//! ```text
//! cargo build --release -p server --example rate_memory
//! /usr/bin/time -l target/release/examples/rate_memory 0        # macOS (-v on Linux)
//! /usr/bin/time -l target/release/examples/rate_memory
//! ```
#![allow(clippy::print_stdout)]

use std::net::{IpAddr, Ipv6Addr};

use server::net::IpKey;
use server::rate::{KeyedLimiter, MAX_KEYS, RateSettings, WS_ANON_UPGRADE_IP};

fn main() {
    let keys = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse::<u64>().ok())
        .unwrap_or(u64::try_from(MAX_KEYS).unwrap_or(u64::MAX));
    let limiter = KeyedLimiter::new(WS_ANON_UPGRADE_IP, &RateSettings::default());
    for n in 0..keys {
        let high = u128::from(0x2001_0db8_0000_0000u64 | n) << 64;
        limiter.check(&IpKey::from(IpAddr::V6(Ipv6Addr::from(high | 1))));
    }
    println!("keys={} capacity={}", limiter.len(), limiter.capacity());
}
