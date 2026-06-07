/*!
`llm-prompt-cache-stats` — track Anthropic prompt-cache hit rates and savings.

Anthropic's [prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)
lets you reuse a large, stable prompt prefix across requests. Each response's
`usage` block reports three token counts:

- `input_tokens` — uncached tokens processed at the full input price.
- `cache_read_input_tokens` — tokens served from the cache at **~0.1×** the
  base input price.
- `cache_creation_input_tokens` — tokens written to the cache. Writes cost
  **1.25×** the base price for the 5-minute TTL and **2×** for the 1-hour TTL.

This crate aggregates those counts across many requests so you can compute hit
rates and dollar savings.

# Quick start

```rust
use llm_prompt_cache_stats::CacheStats;

let mut s = CacheStats::new();
// record(input_tokens, cache_read_tokens, cache_write_tokens) from each response's usage
s.record(1000, 500, 200);
s.record(1000, 900, 0);

assert_eq!(s.request_count(), 2);
assert!(s.hit_rate() > 0.0);

// Gross savings from cache reads alone (full price $3/1M, cache-read $0.30/1M):
let gross = s.estimated_savings_usd(0.003, 0.0003);
assert!(gross > 0.0);
```

# Accounting for cache-write cost

[`CacheStats::estimated_savings_usd`] measures only the upside from cache reads.
Cache *writes* are billed at a premium, so a complete picture subtracts that
premium with [`CacheStats::net_savings_usd`]:

```rust
use llm_prompt_cache_stats::CacheStats;

let mut s = CacheStats::new();
s.record(1000, 0, 1000); // first request: pure cache write, no reads yet
s.record(1000, 1000, 0); // second request: served entirely from cache

// Anthropic prices are per *million* tokens; pass per-1k figures here.
// full = $3/1M, cache-read = $0.30/1M, write multiplier = 1.25 (5-min TTL).
let net = s.net_savings_usd(0.003, 0.0003, 1.25);
let gross = s.estimated_savings_usd(0.003, 0.0003);
assert!(net < gross); // the write premium reduces the net benefit
```
*/

/// Anthropic cache-write cost multiplier for the default 5-minute TTL (1.25× the
/// base input price). Pass to [`CacheStats::net_savings_usd`].
pub const WRITE_MULTIPLIER_5M: f64 = 1.25;

/// Anthropic cache-write cost multiplier for the 1-hour TTL (2× the base input
/// price). Pass to [`CacheStats::net_savings_usd`].
pub const WRITE_MULTIPLIER_1H: f64 = 2.0;

/// Cache statistics for a single request, as reported by one response's `usage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheRecord {
    /// Uncached input tokens, billed at the full input price.
    pub input_tokens: u64,
    /// Tokens served from the cache (`cache_read_input_tokens`).
    pub cache_read_tokens: u64,
    /// Tokens written to the cache (`cache_creation_input_tokens`).
    pub cache_write_tokens: u64,
}

impl CacheRecord {
    /// Tokens served from the cache (a "hit").
    pub fn hit_tokens(&self) -> u64 {
        self.cache_read_tokens
    }

    /// Uncached input tokens (a "miss"), saturating at zero if `cache_read_tokens`
    /// somehow exceeds `input_tokens`.
    pub fn miss_tokens(&self) -> u64 {
        self.input_tokens.saturating_sub(self.cache_read_tokens)
    }

    /// Fraction of input tokens served from the cache, in `[0.0, 1.0]`.
    ///
    /// Returns `0.0` when `input_tokens` is zero rather than dividing by zero.
    pub fn hit_rate(&self) -> f64 {
        if self.input_tokens == 0 {
            0.0
        } else {
            self.cache_read_tokens as f64 / self.input_tokens as f64
        }
    }
}

/// Aggregates cache statistics across multiple requests.
#[derive(Default, Debug, Clone)]
pub struct CacheStats {
    records: Vec<CacheRecord>,
}

impl CacheStats {
    /// Create an empty aggregator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one request's cache metrics, taken from a response's `usage` block:
    /// `input_tokens`, `cache_read_input_tokens`, and `cache_creation_input_tokens`.
    pub fn record(&mut self, input_tokens: u64, cache_read_tokens: u64, cache_write_tokens: u64) {
        self.push(CacheRecord {
            input_tokens,
            cache_read_tokens,
            cache_write_tokens,
        });
    }

    /// Record a pre-built [`CacheRecord`].
    pub fn push(&mut self, record: CacheRecord) {
        self.records.push(record);
    }

    /// Number of recorded requests.
    pub fn request_count(&self) -> usize {
        self.records.len()
    }

    /// Whether any requests have been recorded.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Total uncached input tokens across all requests.
    pub fn total_input_tokens(&self) -> u64 {
        self.records.iter().map(|r| r.input_tokens).sum()
    }

    /// Total cache-read tokens across all requests.
    pub fn total_cache_read_tokens(&self) -> u64 {
        self.records.iter().map(|r| r.cache_read_tokens).sum()
    }

    /// Total cache-write tokens across all requests.
    pub fn total_cache_write_tokens(&self) -> u64 {
        self.records.iter().map(|r| r.cache_write_tokens).sum()
    }

    /// Overall cache hit rate in `[0.0, 1.0]`, weighted by token volume.
    ///
    /// This is `total_cache_read_tokens / total_input_tokens`, so a few large
    /// requests dominate. For an unweighted per-request average, see
    /// [`avg_per_request_hit_rate`](Self::avg_per_request_hit_rate). Returns
    /// `0.0` when no input tokens have been recorded.
    pub fn hit_rate(&self) -> f64 {
        let total = self.total_input_tokens();
        if total == 0 {
            0.0
        } else {
            self.total_cache_read_tokens() as f64 / total as f64
        }
    }

    /// Estimated USD saved by serving tokens from the cache instead of paying the
    /// full input price.
    ///
    /// This is the *gross* upside and ignores the cost of writing the cache. For
    /// a net figure that subtracts the write premium, use
    /// [`net_savings_usd`](Self::net_savings_usd).
    ///
    /// Both prices are per 1,000 tokens. Anthropic publishes prices per *million*
    /// tokens, so divide those by 1,000 first (e.g. `$3/1M` → `0.003`). Cache
    /// reads are billed at roughly 0.1× the base input price.
    ///
    /// `savings = cache_read_tokens × (full_price − cache_read_price)`
    pub fn estimated_savings_usd(
        &self,
        full_price_per_1k: f64,
        cache_read_price_per_1k: f64,
    ) -> f64 {
        let saved_per_token = (full_price_per_1k - cache_read_price_per_1k) / 1000.0;
        self.total_cache_read_tokens() as f64 * saved_per_token
    }

    /// Estimated *net* USD saved, accounting for the cache-write premium.
    ///
    /// Cache reads save `(full − cache_read)` per token, but writing the cache
    /// costs a premium of `(write_multiplier − 1) × full` per token over what the
    /// same tokens would have cost uncached. The 5-minute TTL multiplier is
    /// `1.25` ([`WRITE_MULTIPLIER_5M`]); the 1-hour TTL multiplier is `2.0`
    /// ([`WRITE_MULTIPLIER_1H`]).
    ///
    /// `net = read_savings − write_premium`
    ///
    /// The result can be negative: if a cached prefix is written but rarely read
    /// back before it expires, caching costs more than it saves.
    ///
    /// `full_price_per_1k` and `cache_read_price_per_1k` are per 1,000 tokens.
    pub fn net_savings_usd(
        &self,
        full_price_per_1k: f64,
        cache_read_price_per_1k: f64,
        write_multiplier: f64,
    ) -> f64 {
        let read_savings = self.estimated_savings_usd(full_price_per_1k, cache_read_price_per_1k);
        let write_premium_per_token = (write_multiplier - 1.0) * full_price_per_1k / 1000.0;
        let write_cost = self.total_cache_write_tokens() as f64 * write_premium_per_token;
        read_savings - write_cost
    }

    /// Unweighted mean of each request's [`CacheRecord::hit_rate`].
    ///
    /// Unlike [`hit_rate`](Self::hit_rate), every request counts equally
    /// regardless of size. Returns `0.0` when no requests have been recorded.
    pub fn avg_per_request_hit_rate(&self) -> f64 {
        if self.records.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.records.iter().map(|r| r.hit_rate()).sum();
        sum / self.records.len() as f64
    }

    /// All recorded requests, in insertion order.
    pub fn records(&self) -> &[CacheRecord] {
        &self.records
    }

    /// Remove all recorded requests, keeping the allocated capacity.
    pub fn clear(&mut self) {
        self.records.clear();
    }
}

impl Extend<CacheRecord> for CacheStats {
    fn extend<T: IntoIterator<Item = CacheRecord>>(&mut self, iter: T) {
        self.records.extend(iter);
    }
}

impl FromIterator<CacheRecord> for CacheStats {
    fn from_iter<T: IntoIterator<Item = CacheRecord>>(iter: T) -> Self {
        Self {
            records: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_count() {
        let mut s = CacheStats::new();
        s.record(1000, 500, 200);
        assert_eq!(s.request_count(), 1);
    }

    #[test]
    fn total_input_tokens() {
        let mut s = CacheStats::new();
        s.record(1000, 0, 0);
        s.record(500, 0, 0);
        assert_eq!(s.total_input_tokens(), 1500);
    }

    #[test]
    fn hit_rate() {
        let mut s = CacheStats::new();
        s.record(1000, 500, 0); // 50% hit rate
        assert!((s.hit_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn hit_rate_zero() {
        let mut s = CacheStats::new();
        s.record(1000, 0, 0);
        assert_eq!(s.hit_rate(), 0.0);
    }

    #[test]
    fn hit_rate_full() {
        let mut s = CacheStats::new();
        s.record(1000, 1000, 0);
        assert!((s.hit_rate() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn savings_estimation() {
        let mut s = CacheStats::new();
        s.record(1000, 1000, 0); // all cached
                                 // full price $0.015/1k, cache read $0.0015/1k → savings $0.0135/1k
        let savings = s.estimated_savings_usd(0.015, 0.0015);
        assert!((savings - 0.0135).abs() < 1e-9);
    }

    #[test]
    fn savings_no_cache_hits() {
        let mut s = CacheStats::new();
        s.record(1000, 0, 0);
        assert_eq!(s.estimated_savings_usd(0.015, 0.0015), 0.0);
    }

    #[test]
    fn avg_per_request_hit_rate() {
        let mut s = CacheStats::new();
        s.record(1000, 1000, 0); // 1.0
        s.record(1000, 0, 0); // 0.0
        assert!((s.avg_per_request_hit_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn is_empty() {
        let s = CacheStats::new();
        assert!(s.is_empty());
    }

    #[test]
    fn clear() {
        let mut s = CacheStats::new();
        s.record(100, 50, 0);
        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn cache_record_hit_miss() {
        let r = CacheRecord {
            input_tokens: 1000,
            cache_read_tokens: 600,
            cache_write_tokens: 400,
        };
        assert_eq!(r.hit_tokens(), 600);
        assert_eq!(r.miss_tokens(), 400);
    }

    #[test]
    fn multiple_records_aggregate() {
        let mut s = CacheStats::new();
        s.record(1000, 200, 100);
        s.record(1000, 800, 50);
        assert_eq!(s.total_cache_read_tokens(), 1000);
        assert_eq!(s.total_cache_write_tokens(), 150);
    }

    #[test]
    fn miss_tokens_saturates_on_over_read() {
        // cache_read_tokens > input_tokens must not underflow.
        let r = CacheRecord {
            input_tokens: 100,
            cache_read_tokens: 250,
            cache_write_tokens: 0,
        };
        assert_eq!(r.miss_tokens(), 0);
    }

    #[test]
    fn empty_stats_metrics_are_zero() {
        let s = CacheStats::new();
        assert_eq!(s.request_count(), 0);
        assert_eq!(s.total_input_tokens(), 0);
        assert_eq!(s.hit_rate(), 0.0);
        assert_eq!(s.avg_per_request_hit_rate(), 0.0);
        assert_eq!(s.estimated_savings_usd(0.003, 0.0003), 0.0);
        assert_eq!(s.net_savings_usd(0.003, 0.0003, WRITE_MULTIPLIER_5M), 0.0);
    }

    #[test]
    fn weighted_vs_unweighted_hit_rate_differ() {
        let mut s = CacheStats::new();
        s.record(100, 100, 0); // small request, 100% hit
        s.record(900, 0, 0); // large request, 0% hit
                             // Weighted: 100 / 1000 = 0.1
        assert!((s.hit_rate() - 0.1).abs() < 1e-9);
        // Unweighted: (1.0 + 0.0) / 2 = 0.5
        assert!((s.avg_per_request_hit_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn net_savings_subtracts_write_premium() {
        let mut s = CacheStats::new();
        s.record(0, 0, 1000); // wrote 1000 tokens to cache
        s.record(1000, 1000, 0); // read 1000 tokens back

        // full = 0.003/1k, cache_read = 0.0003/1k.
        // gross read savings = 1000 * (0.003 - 0.0003)/1000 = 0.0027
        let gross = s.estimated_savings_usd(0.003, 0.0003);
        assert!((gross - 0.0027).abs() < 1e-9);

        // write premium (5-min TTL, 1.25x) = 1000 * (1.25 - 1) * 0.003/1000 = 0.00075
        // net = 0.0027 - 0.00075 = 0.00195
        let net = s.net_savings_usd(0.003, 0.0003, WRITE_MULTIPLIER_5M);
        assert!((net - 0.00195).abs() < 1e-9);
        assert!(net < gross);
    }

    #[test]
    fn net_savings_can_be_negative_when_writes_dominate() {
        let mut s = CacheStats::new();
        // Large write, no reads back: caching was a net loss.
        s.record(0, 0, 10_000);
        let net = s.net_savings_usd(0.003, 0.0003, WRITE_MULTIPLIER_1H);
        assert!(net < 0.0, "expected negative net savings, got {net}");
    }

    #[test]
    fn write_multiplier_constants() {
        assert_eq!(WRITE_MULTIPLIER_5M, 1.25);
        assert_eq!(WRITE_MULTIPLIER_1H, 2.0);
    }

    #[test]
    fn push_and_records_round_trip() {
        let mut s = CacheStats::new();
        let rec = CacheRecord {
            input_tokens: 10,
            cache_read_tokens: 4,
            cache_write_tokens: 2,
        };
        s.push(rec.clone());
        assert_eq!(s.records(), &[rec]);
    }

    #[test]
    fn from_iter_and_extend() {
        let recs = vec![
            CacheRecord {
                input_tokens: 100,
                cache_read_tokens: 50,
                cache_write_tokens: 0,
            },
            CacheRecord {
                input_tokens: 100,
                cache_read_tokens: 100,
                cache_write_tokens: 0,
            },
        ];
        let mut s: CacheStats = recs.iter().cloned().collect();
        assert_eq!(s.request_count(), 2);
        assert_eq!(s.total_cache_read_tokens(), 150);

        s.extend(recs);
        assert_eq!(s.request_count(), 4);
        assert_eq!(s.total_cache_read_tokens(), 300);
    }
}
