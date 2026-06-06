# llm-prompt-cache-stats

A small, dependency-free Rust library for tracking [Anthropic prompt cache](https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching) hit rates and estimating the cost savings they produce.

When you use prompt caching with the Claude API, each response reports how many input tokens were served from cache (`cache_read`), written to cache (`cache_write`), and processed normally. This crate aggregates those per-request numbers so you can answer questions like *"what is my overall cache hit rate?"* and *"how many dollars has caching saved me?"*

## What it does

- Record per-request token counts (`input`, `cache_read`, `cache_write`).
- Compute overall and per-request cache hit rates.
- Aggregate total input / cache-read / cache-write tokens across many requests.
- Estimate USD savings given your full and cached input prices.

## Install

This crate has no runtime dependencies. Add it to your `Cargo.toml`:

```toml
[dependencies]
llm-prompt-cache-stats = "0.1"
```

Or, if you are working against this repository directly:

```toml
[dependencies]
llm-prompt-cache-stats = { git = "https://github.com/MukundaKatta/llm-prompt-cache-stats-rs" }
```

## Usage

```rust
use llm_prompt_cache_stats::CacheStats;

let mut stats = CacheStats::new();

// record(input_tokens, cache_read_tokens, cache_write_tokens)
stats.record(1000, 500, 200);
stats.record(1000, 800, 0);

println!("requests:        {}", stats.request_count());
println!("hit rate:        {:.1}%", stats.hit_rate() * 100.0);
println!("avg per-request: {:.1}%", stats.avg_per_request_hit_rate() * 100.0);

// Estimate savings: full input price vs. cache-read price, per 1k tokens.
// Anthropic cache reads are roughly 10% of the full input price.
let saved = stats.estimated_savings_usd(0.015, 0.0015);
println!("estimated saved: ${:.4}", saved);
```

## API overview

### `CacheStats`

Aggregates statistics across multiple requests.

| Method | Description |
| --- | --- |
| `new()` | Create an empty collector. |
| `record(input, cache_read, cache_write)` | Add one request's token counts. |
| `request_count()` / `is_empty()` | Number of recorded requests. |
| `total_input_tokens()` | Sum of input tokens. |
| `total_cache_read_tokens()` | Sum of cache-read tokens. |
| `total_cache_write_tokens()` | Sum of cache-write tokens. |
| `hit_rate()` | Overall hit rate in `[0.0, 1.0]` (cache reads / total input). |
| `avg_per_request_hit_rate()` | Mean of each request's individual hit rate. |
| `estimated_savings_usd(full_price_per_1k, cache_read_price_per_1k)` | Estimated USD saved by cache reads. |
| `records()` | Borrow the underlying `CacheRecord` slice. |
| `clear()` | Drop all recorded requests. |

### `CacheRecord`

A single request's metrics, with `hit_tokens()`, `miss_tokens()`, and `hit_rate()` helpers.

## Building and testing

```bash
cargo build
cargo test
```

## Tech stack

- Rust (edition 2021), no external dependencies.

## License

MIT
