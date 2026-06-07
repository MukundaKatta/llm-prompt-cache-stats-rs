# llm-prompt-cache-stats

Track [Anthropic prompt-cache](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)
hit rates and dollar savings from your application's request stream.

Anthropic's prompt caching reuses a large, stable prompt prefix across requests.
Every response's `usage` block reports three token counts:

| `usage` field                  | Meaning                              | Billing                              |
| ------------------------------ | ------------------------------------ | ------------------------------------ |
| `input_tokens`                 | Uncached tokens                      | Full input price                     |
| `cache_read_input_tokens`      | Tokens served from the cache         | ~0.1× the base input price           |
| `cache_creation_input_tokens`  | Tokens written to the cache          | 1.25× (5-min TTL) / 2× (1-hour TTL)  |

Feed those three numbers in per request and this crate aggregates them into hit
rates and savings — including the cache-write premium, which a naive "reads are
cheap" calculation ignores.

## Install

This crate has no dependencies. Add it to your `Cargo.toml`:

```toml
[dependencies]
llm-prompt-cache-stats = "0.1"
```

Or, while it is unpublished, point at the Git repository:

```toml
[dependencies]
llm-prompt-cache-stats = { git = "https://github.com/MukundaKatta/llm-prompt-cache-stats-rs" }
```

## Usage

```rust
use llm_prompt_cache_stats::{CacheStats, WRITE_MULTIPLIER_5M};

let mut stats = CacheStats::new();

// record(input_tokens, cache_read_tokens, cache_write_tokens) from each
// response's `usage` block:
stats.record(500, 0, 8000);     // turn 1: writes the 8k-token prefix
stats.record(200, 8000, 0);     // turn 2: prefix served from cache
stats.record(200, 8000, 0);     // turn 3: ...and again

println!("requests:        {}", stats.request_count());
println!("hit rate:        {:.1}%", stats.hit_rate() * 100.0);
println!("cache reads:     {}", stats.total_cache_read_tokens());

// Anthropic prices are per *million* tokens; pass per-1k figures
// (e.g. $3/1M input -> 0.003, $0.30/1M cache read -> 0.0003).
let gross = stats.estimated_savings_usd(0.003, 0.0003);
let net   = stats.net_savings_usd(0.003, 0.0003, WRITE_MULTIPLIER_5M);

println!("gross savings:   ${gross:.4}");  // cache reads only
println!("net savings:     ${net:.4}");    // minus the cache-write premium
```

## API

### `CacheRecord`

One request's cache metrics.

| Method            | Description                                                   |
| ----------------- | ------------------------------------------------------------ |
| `hit_tokens()`    | Tokens served from the cache.                                |
| `miss_tokens()`   | Uncached input tokens (saturates at 0).                      |
| `hit_rate()`      | `cache_read_tokens / input_tokens`, in `[0.0, 1.0]`.         |

### `CacheStats`

Aggregator over many `CacheRecord`s.

| Method                                                              | Description                                                                 |
| ------------------------------------------------------------------ | --------------------------------------------------------------------------- |
| `new()`                                                            | Empty aggregator.                                                           |
| `record(input, read, write)`                                      | Append one request's metrics.                                              |
| `push(CacheRecord)`                                               | Append a pre-built record.                                                  |
| `request_count()` / `is_empty()`                                  | Number of recorded requests.                                               |
| `total_input_tokens()`                                            | Sum of uncached input tokens.                                              |
| `total_cache_read_tokens()`                                       | Sum of cache-read tokens.                                                  |
| `total_cache_write_tokens()`                                      | Sum of cache-write tokens.                                                 |
| `hit_rate()`                                                      | Token-weighted overall hit rate.                                          |
| `avg_per_request_hit_rate()`                                      | Unweighted mean of per-request hit rates.                                 |
| `estimated_savings_usd(full, read)`                              | Gross USD saved by cache reads.                                           |
| `net_savings_usd(full, read, write_multiplier)`                 | Net USD saved, subtracting the cache-write premium. Can be negative.     |
| `records()` / `clear()`                                          | Borrow / drop the recorded requests.                                      |

`CacheStats` also implements `FromIterator<CacheRecord>` and
`Extend<CacheRecord>`, so you can `collect()` records into it.

### Constants

- `WRITE_MULTIPLIER_5M` = `1.25` — cache-write cost multiplier for the 5-minute TTL.
- `WRITE_MULTIPLIER_1H` = `2.0` — cache-write cost multiplier for the 1-hour TTL.

## Development

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy -- -D warnings
```

## License

MIT
