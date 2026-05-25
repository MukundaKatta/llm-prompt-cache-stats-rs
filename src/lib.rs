/*!
llm-prompt-cache-stats: track Anthropic prompt cache hit rates and savings.

```rust
use llm_prompt_cache_stats::CacheStats;

let mut s = CacheStats::new();
s.record(1000, 500, 200);  // input, cache_read, cache_write tokens
assert!(s.hit_rate() > 0.0);
```
*/

/// Statistics for a single request.
#[derive(Debug, Clone)]
pub struct CacheRecord {
    pub input_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl CacheRecord {
    pub fn hit_tokens(&self) -> u64 { self.cache_read_tokens }
    pub fn miss_tokens(&self) -> u64 { self.input_tokens.saturating_sub(self.cache_read_tokens) }
    pub fn hit_rate(&self) -> f64 {
        if self.input_tokens == 0 { 0.0 } else { self.cache_read_tokens as f64 / self.input_tokens as f64 }
    }
}

/// Aggregates cache statistics across multiple requests.
#[derive(Default, Debug)]
pub struct CacheStats {
    records: Vec<CacheRecord>,
}

impl CacheStats {
    pub fn new() -> Self { Self::default() }

    /// Record a request's cache metrics.
    pub fn record(&mut self, input_tokens: u64, cache_read_tokens: u64, cache_write_tokens: u64) {
        self.records.push(CacheRecord { input_tokens, cache_read_tokens, cache_write_tokens });
    }

    pub fn request_count(&self) -> usize { self.records.len() }
    pub fn is_empty(&self) -> bool { self.records.is_empty() }

    /// Total input tokens across all requests.
    pub fn total_input_tokens(&self) -> u64 { self.records.iter().map(|r| r.input_tokens).sum() }

    /// Total cache-read tokens.
    pub fn total_cache_read_tokens(&self) -> u64 { self.records.iter().map(|r| r.cache_read_tokens).sum() }

    /// Total cache-write tokens.
    pub fn total_cache_write_tokens(&self) -> u64 { self.records.iter().map(|r| r.cache_write_tokens).sum() }

    /// Overall cache hit rate [0.0, 1.0].
    pub fn hit_rate(&self) -> f64 {
        let total = self.total_input_tokens();
        if total == 0 { 0.0 } else { self.total_cache_read_tokens() as f64 / total as f64 }
    }

    /// Estimated USD saved by cache reads.
    /// Anthropic cache reads are ~10% of full input price.
    /// Savings = cache_read_tokens * (full_price - cache_read_price)
    pub fn estimated_savings_usd(&self, full_price_per_1k: f64, cache_read_price_per_1k: f64) -> f64 {
        let saved_per_token = (full_price_per_1k - cache_read_price_per_1k) / 1000.0;
        self.total_cache_read_tokens() as f64 * saved_per_token
    }

    /// Average hit rate per request.
    pub fn avg_per_request_hit_rate(&self) -> f64 {
        if self.records.is_empty() { return 0.0; }
        let sum: f64 = self.records.iter().map(|r| r.hit_rate()).sum();
        sum / self.records.len() as f64
    }

    pub fn records(&self) -> &[CacheRecord] { &self.records }
    pub fn clear(&mut self) { self.records.clear(); }
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
        s.record(1000, 500, 0);  // 50% hit rate
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
        s.record(1000, 0, 0);    // 0.0
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
        let r = CacheRecord { input_tokens: 1000, cache_read_tokens: 600, cache_write_tokens: 400 };
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
}
