# Adding a provider

A provider is one Rust file in `crates/quotis-core/src/providers/` and one line in `registry()`. Settings, the overlay and the CLI pick it up from the registry; no UI change is needed.

## Before writing code

Answer these and put the answers in [PROVIDER_MATRIX.md](PROVIDER_MATRIX.md):

1. What does the provider actually expose? An allowance, a balance, consumption only, or nothing?
2. Where from: a documented API, an endpoint the vendor's own tool uses, or local files?
3. How does the user authenticate, and is third-party use of that credential permitted by the vendor's terms?
4. How often may it be asked?
5. Can you test it on a real account?

If the honest answer to 1 is "nothing reliable", list the provider as not supported. Do not build an estimate that looks like a quota.

## Rules

- **Never invent an allowance.** Token counts from local logs are `Category::LocalUsage`. Only a number the provider reported as an allowance is `Category::ProviderQuota`.
- **Never assume a window.** Set `window_seconds` only from data the provider gave, or from a window the provider names (such as a `seven_day` field).
- **Credentials stay put.** Read a credential at request time, send it only to that provider's own host over HTTPS, never store, log or return it.
- **Read-only.** Open databases with `open_ro`. Never write to another tool's files.
- **Bounded.** Use `fsx::read_small`, `fsx::read_tail`, `fsx::read_new_lines` and `fsx::jsonl_files`. Do not call `std::fs::read_to_string` on a tool's log.
- **Fail loudly.** A response that lacks the expected fields is `net::bad_response()`. A failed query is an error, never a zero.
- **Clean strings.** Pass anything provider-supplied through `model::clean`.
- **Opt-in for user-supplied keys.** Set `default_enabled: false` and `key_env`.

## Skeleton

```rust
use super::{Info, Provider};
use crate::model::{Collected, Metric, ProviderError, Source};
use crate::net::{bad_response, get, not_signed_in};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct Example;

static INFO: Info = Info {
    id: "example",                       // lowercase letters only
    name: "Example",
    vendor: "Example Inc",
    color: "#888888",
    hint: "What the user must do, and what they will see.",
    endpoint: Some("api.example.com"),   // None for local-only providers
    documented: true,                    // is that endpoint in the vendor's public docs?
    key_env: None,
    default_enabled: true,
};

/// Pure parser: the part that gets the tests.
pub fn parse(v: &Value, now: i64) -> Option<Collected> {
    let used = v.get("used_percent")?.as_f64()?;
    let resets = v.get("resets_at").and_then(Value::as_i64);
    Some(Collected { metrics: vec![Metric::quota_percent("plan", "plan", used, resets, Source::ProviderApi, now)], ..Default::default() })
}

impl Provider for Example {
    fn info(&self) -> &'static Info { &INFO }
    fn default_dir(&self) -> PathBuf { crate::settings::home().join(".example") }
    fn detect(&self, dir: &Path) -> bool { dir.join("auth.json").is_file() }   // no network here

    fn collect_online(&self, dir: &Path, now: i64) -> Option<Result<Collected, ProviderError>> {
        Some((|| {
            let auth: Value = serde_json::from_str(&crate::fsx::read_small(&dir.join("auth.json"))?).map_err(|_| not_signed_in())?;
            let token = auth.get("token").and_then(Value::as_str).ok_or_else(not_signed_in)?;
            let v = get("https://api.example.com/usage", &[("Authorization", &format!("Bearer {token}"))])?;
            parse(&v, now).ok_or_else(bad_response)
        })())
    }
}
```

Then in `providers/mod.rs`: add `mod example;` and `Arc::new(example::Example)` to `registry()`.

For a local-log provider implement `collect_local` and `watch_dir` instead; `tokenlog.rs` is the model. For a provider with both, see `codex.rs`: the engine merges the two by metric id and keeps the newer observation.

## Tests every adapter needs

Put them in the adapter's file. Use fixtures; never a real account.

- The documented or recorded happy-path response.
- A response missing the expected fields returns `None` (so the engine reports `bad_response`).
- Hostile strings: markup and control characters in any text field.
- Timestamps with a non-zero UTC offset, and a malformed one.
- Missing or malformed credentials give `NotSignedIn`, and the fake credential never appears in the error.
- For local sources: an appended file, a truncated file, an empty folder.

The registry tests in `providers/mod.rs` then check your `Info` automatically: unique id, API-key providers are opt-in, nothing detected in an empty folder, and no network call without a login.

A live check belongs outside CI: run `quotis-cli diagnose --provider example --live` on an account you are allowed to test with, compare against the vendor's own usage page, and record the result in the matrix.

## Error kinds

| Kind | Use it when | What the user sees |
|---|---|---|
| `NotSignedIn` | No credential found | Provider hidden ("not detected") |
| `AuthExpired` | HTTP 401/403, or an expired token | "login expired" |
| `RateLimited` | HTTP 429 | Last numbers kept, marked stale; backoff |
| `Offline` | The request could not be made | Last numbers kept; "Offline" if every provider is |
| `Unavailable` | Other HTTP status, unreadable local data | Last numbers kept, or an error row |
| `BadResponse` | Unexpected shape | Error row |
| `PermissionDenied` | The OS refused access | "No access" |
