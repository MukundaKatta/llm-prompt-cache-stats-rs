//! Integration tests exercising the public API as an external crate would.

use llm_prompt_cache_stats::{CacheRecord, CacheStats, WRITE_MULTIPLIER_1H, WRITE_MULTIPLIER_5M};

/// A realistic session: a large prompt prefix is written once, then read back
/// across several follow-up requests — the canonical caching win.
#[test]
fn realistic_session_is_net_positive() {
    let mut s = CacheStats::new();
    // Turn 1: write the 8k-token prefix, nothing to read yet.
    s.record(500, 0, 8000);
    // Turns 2-5: prefix served from cache, only the new question is uncached.
    for _ in 0..4 {
        s.record(200, 8000, 0);
    }

    assert_eq!(s.request_count(), 5);
    assert_eq!(s.total_cache_write_tokens(), 8000);
    assert_eq!(s.total_cache_read_tokens(), 32_000);

    // Anthropic Sonnet-class pricing: $3/1M input, $0.30/1M cache read.
    let full = 0.003;
    let read = 0.0003;

    let gross = s.estimated_savings_usd(full, read);
    let net = s.net_savings_usd(full, read, WRITE_MULTIPLIER_5M);

    assert!(gross > 0.0);
    assert!(net > 0.0);
    assert!(net < gross, "net must be below gross by the write premium");

    // gross = 32000 * (0.003 - 0.0003) / 1000 = 0.0864
    assert!((gross - 0.0864).abs() < 1e-9);
    // write premium = 8000 * 0.25 * 0.003 / 1000 = 0.006
    // net = 0.0864 - 0.006 = 0.0804
    assert!((net - 0.0804).abs() < 1e-9);
}

/// Building a CacheStats from an iterator of records aggregates correctly.
#[test]
fn collect_from_records() {
    let records = (0..3).map(|_| CacheRecord {
        input_tokens: 100,
        cache_read_tokens: 100,
        cache_write_tokens: 0,
    });
    let s: CacheStats = records.collect();

    assert_eq!(s.request_count(), 3);
    assert!((s.hit_rate() - 1.0).abs() < 1e-9);
    assert!((s.avg_per_request_hit_rate() - 1.0).abs() < 1e-9);
}

/// A write that is never read back before expiry is a net loss at the 1-hour TTL.
#[test]
fn unused_one_hour_cache_is_a_loss() {
    let mut s = CacheStats::new();
    s.record(0, 0, 5000);
    let net = s.net_savings_usd(0.003, 0.0003, WRITE_MULTIPLIER_1H);
    assert!(net < 0.0);
}
