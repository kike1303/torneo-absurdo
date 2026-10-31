//! 010 RF7: what anyone can do with the body of `GET /api/fights/{id}`, with nothing of the server:
//! the source of this file names no module of it (`fights_public_verify.rs` checks that).
//!
//! In a `decided` fight every step is required: a missing `seed`, `spec_*` or `log` is a failed
//! verification, never a step skipped (security review R3).

use engine::{CharacterSpec, FightLog, Ruleset, Seed, canonical_json, simulate};
use protocol::{FightDetail, FightFinal, FightState, VoidReason};
use sha2::{Digest, Sha256};

fn hex_decode(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err("odd hex".to_owned());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "bad hex".to_owned()))
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// RF2, by concatenation, as any language would.
fn snapshot_of(version: u16, a: &str, b: &str) -> String {
    let text = format!(r#"{{"ruleset_version":{version},"a":{a},"b":{b}}}"#);
    hex(&Sha256::digest(text.as_bytes()))
}

fn check_commit(detail: &FightDetail) -> Result<[u8; 32], String> {
    let seed = detail.seed.as_deref().ok_or("no seed")?;
    let commit = detail.fight.seed_commit.as_deref().ok_or("no commit")?;
    let bytes = hex_decode(seed)?;
    if hex(&Sha256::digest(&bytes)) != commit {
        return Err("the seed does not match the commit".to_owned());
    }
    bytes
        .try_into()
        .map_err(|_| "a seed of the wrong size".to_owned())
}

fn check_snapshot(detail: &FightDetail) -> Result<(), String> {
    let a = detail.spec_a.as_deref().ok_or("no spec_a")?;
    let b = detail.spec_b.as_deref().ok_or("no spec_b")?;
    if snapshot_of(detail.fight.ruleset_version, a, b) != detail.fight.snapshot_sha256 {
        return Err("the snapshot does not match its hash".to_owned());
    }
    Ok(())
}

/// Verifies a settled fight, or says why it does not verify.
pub fn verify(detail: &FightDetail) -> Result<(), String> {
    let fight = &detail.fight;
    if fight.state != FightState::Settled {
        return Err("not settled".to_owned());
    }
    match fight.final_result {
        None => Err("no final".to_owned()),
        Some(FightFinal::Cancelled { .. }) => {
            if detail.seed.is_some() || detail.log.is_some() || fight.seed_commit.is_some() {
                return Err("a cancelled fight with a seed or a log".to_owned());
            }
            // A corrupt snapshot may be why it could not run.
            if !matches!(
                fight.final_result,
                Some(FightFinal::Cancelled {
                    reason: protocol::CancelReason::FightUnrunnable
                })
            ) {
                check_snapshot(detail)?;
            }
            Ok(())
        }
        Some(FightFinal::Voided { reason }) => {
            check_commit(detail)?;
            if detail.log.is_some() {
                return Err("a voided fight with a log".to_owned());
            }
            if reason != VoidReason::SnapshotInvalid {
                check_snapshot(detail)?;
            }
            Ok(())
        }
        Some(FightFinal::Decided { winner, reason }) => {
            let seed = check_commit(detail)?;
            check_snapshot(detail)?;
            let (spec_a, spec_b) = (
                detail.spec_a.as_deref().ok_or("no spec_a")?,
                detail.spec_b.as_deref().ok_or("no spec_b")?,
            );
            let log_text = detail.log.as_deref().ok_or("no log")?;
            let ruleset = Ruleset::for_version(fight.ruleset_version).ok_or("unknown version")?;
            let a: CharacterSpec = serde_json::from_str(spec_a).map_err(|_| "spec_a")?;
            let b: CharacterSpec = serde_json::from_str(spec_b).map_err(|_| "spec_b")?;
            let log =
                simulate(&Seed::from_bytes(seed), &a, &b, &ruleset).map_err(|_| "simulate")?;
            let bytes = canonical_json(&log).map_err(|_| "encode")?;
            if bytes != log_text.as_bytes() {
                return Err("the log does not reproduce, byte by byte".to_owned());
            }
            // The bindings a browser would run give the same bytes.
            let wasm = engine_wasm::run(&hex(&seed), spec_a, spec_b, fight.ruleset_version)
                .map_err(|_| "engine_wasm")?;
            if wasm.as_bytes() != log_text.as_bytes() {
                return Err("the bindings disagree".to_owned());
            }
            let published: FightLog = serde_json::from_str(log_text).map_err(|_| "log")?;
            if published.outcome() != Some((winner, reason)) {
                return Err("the final is not the one of the log".to_owned());
            }
            if fight.a.slug != a.slug || fight.b.slug != b.slug {
                return Err("a slug differs from its spec".to_owned());
            }
            if fight.a.slug != published.a.slug
                || fight.b.slug != published.b.slug
                || fight.a.max_hp != published.a.max_hp
                || fight.b.max_hp != published.b.max_hp
                || fight.ruleset_version != published.ruleset_version
            {
                return Err("the summary differs from the log".to_owned());
            }
            if fight.time_limit_ms != ruleset.time_limit_ms()
                || fight.max_duration_ms != ruleset.time_limit_ms() + ruleset.end_ms()
            {
                return Err("the bounds differ from the ruleset".to_owned());
            }
            Ok(())
        }
    }
}
