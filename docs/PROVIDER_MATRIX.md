# Provider matrix

## Release-hardening verification record, 2026-11-16

No authorized real test accounts were supplied. The previous session's denied credential
access was preserved; no real login, token, usage page or Keychain item was read. Thus
**every adapter is fixture-tested, live-UNVERIFIED**, including carried-over integrations.
The CLI command run in a sealed fixture environment is not an account comparison.

| Provider | Live connection | Fixture metrics/source | Reset information | Actual display comparison | Error/limitations |
|---|---|---|---|---|---|
| Claude Code | UNVERIFIED | Quota percent/model windows; parsed API fixture | Reported5h/week/model resets | UNVERIFIED | Undocumented endpoint; macOS Keychain UNVERIFIED |
| Codex | UNVERIFIED | Primary/secondary quota, API/local-log fixtures | Length/reset from data | UNVERIFIED | Undocumented endpoint; no API-key plan quota |
| Copilot | UNVERIFIED | Premium-request quota API fixture | Reported cycle end | UNVERIFIED | github.com only; at most3 distinct auth attempts |
| Cursor | UNVERIFIED | Plan allowance API/read-only SQLite fixtures | Reported cycle end | UNVERIFIED | Undocumented endpoint; no inferred overage |
| Gemini CLI | UNVERIFIED | Local-log5h/24h token fixtures | No allowance/reset | UNVERIFIED | Unknown/malformed input qualifies lower bound; caps also qualify |
| Qwen Code | UNVERIFIED | Local-log5h/24h token fixtures | No allowance/reset | UNVERIFIED | Same coverage limits as Gemini |
| opencode | UNVERIFIED | Local-db tokens/cost estimate fixtures | No allowance/reset | UNVERIFIED | Schema/read failures are errors; cost is not a bill |
| OpenRouter | UNVERIFIED | Consumption/key-limit API fixture | Only reported budget reset | UNVERIFIED | Opt-in; no key/account access authorized |
| DeepSeek | UNVERIFIED | Currency balances API fixture | None | UNVERIFIED | Opt-in; no key/account access authorized |

Fixture/error evidence includes API shape validation, loopback HTTP errors/Retry-After,
expired synthetic logins, stale/reset display, future observations, malformed records,
listing errors, persistence and bounded workers. This is parser/state evidence, not a
current provider contract guarantee. Never advertise unsupported services as working.

Owner comparison procedure: explicitly authorize one provider/account at a time; run
`quotis-cli status --live --json` using only those enabled providers; record UTC timestamp,
connection/error, exact metric source/window/reset/observed_at/qualification and provider
screen reading at the same time. Compare rounding and timestamps, record disagreement,
and redact account/plan data if sensitive. Stop on permission denial; never paste tokens.

---

What Quotis 2.0 reports for each provider, where the number comes from, and how far it has been verified. Accuracy comes before provider count: anything that could not be integrated reliably is listed as not supported.

## Verification levels

- **Fixture-tested:** the parser is tested against recorded or documented response shapes. This proves the parsing, not that the live service still answers that way.
- **Carried from 0.2:** the request and response handling is the same as in released 0.2.x, re-tested with fixtures. Not re-run against a live account during 2.0 development.
- **Unverified live:** built from the vendor's documentation and never run against a real account.

No provider was exercised against a live account during 2.0 development. Before release, each supported provider needs one live check on an account that has it (`quotis-cli status --live`).

## Supported

| Provider | Metrics | Category | Source | Endpoint documented? | Reset time | Verification |
|---|---|---|---|---|---|---|
| Claude Code | 5-hour and 7-day utilisation; per-model 7-day windows when the response includes them | Provider quota (percent) | `api.anthropic.com/api/oauth/usage` with the Claude Code login | No | Yes, when reported | Carried from 0.2, fixture-tested |
| Codex | Primary and secondary rate-limit windows; window length from the data | Provider quota (percent) | `chatgpt.com/backend-api/wham/usage`, plus local session logs for instant updates | No (online); local logs are the tool's own files | Yes | Carried from 0.2, fixture-tested |
| GitHub Copilot | Premium requests remaining | Provider quota (percent) | `api.github.com/copilot_internal/user` with the editor plugin's login | No | Yes (monthly date) | Carried from 0.2, fixture-tested |
| Cursor | Included plan usage this billing cycle | Provider quota (percent) | `cursor.com/api/usage-summary` with the Cursor app login | No | Yes (cycle end) | Carried from 0.2, fixture-tested |
| Gemini CLI | Tokens in the last 5 h and 24 h | Local usage | `~/.gemini/tmp/**/*.jsonl` | Local files | None: no allowance is known | Carried from 0.2, fixture-tested |
| Qwen Code | Tokens in the last 5 h and 24 h | Local usage | `~/.qwen/tmp/**/*.jsonl` | Local files | None | Carried from 0.2, fixture-tested |
| opencode | Tokens and cost in the last 5 h and 24 h | Local usage; cost is an estimate | `opencode.db` (read-only) | Local files | None | Carried from 0.2, fixture-tested |
| OpenRouter (opt-in) | Credits used today and this month; key limit and budget when set; free-model requests left today | API consumption; provider quota when a limit exists | `GET https://openrouter.ai/api/v1/key` with `OPENROUTER_API_KEY` | **Yes** | Budget reset when reported | **Unverified live** |
| DeepSeek (opt-in) | Prepaid balance per currency | Provider quota (balance) | `GET https://api.deepseek.com/user/balance` with `DEEPSEEK_API_KEY` | **Yes** | None: a balance does not reset | **Unverified live** |

### Known limits of the supported set

- **Claude Code:** API-key logins have no plan limits; the row shows "not signed in". Quotis never refreshes an expired login; opening Claude Code does.
- **Codex:** with an API-key login only the local logs are used, and they show the limits as of the last Codex message. If the data carries no window length the metric is labelled "primary" / "secondary" rather than guessing "5h" / "weekly".
- **Copilot:** only github.com logins are used. GitHub Enterprise Server is not supported. Unlimited plans show a note, not a bar.
- **Cursor:** plans with no included usage show a note.
- **Gemini CLI, Qwen Code:** local counts do not imply an allowance. Unknown log formats
  and incomplete coverage are disclosed as partial lower bounds, including zero counts.
- **opencode:** the cost is opencode's own estimate from model prices.
- **OpenRouter:** a key with no limit shows usage only; no allowance is invented.

## Not supported in 2.0

Assessed from each product's public behaviour as known at the time of writing. These assessments were **not** re-researched against current vendor documentation one by one; treat them as the starting point for the next integration round, not as findings.

| Candidate | Assessment | What would make it feasible |
|---|---|---|
| OpenAI API | Usage and cost endpoints exist but need an organisation admin key | An opt-in adapter taking an admin key from the environment |
| Anthropic API | Usage and cost reporting needs an admin API key | Same |
| Gemini API | Quotas live in Google Cloud; no simple per-key remaining-quota endpoint known | Cloud Monitoring with user-granted OAuth |
| Azure OpenAI, AWS Bedrock, Google Vertex AI | Quotas and spend live in each cloud's monitoring and billing APIs, behind cloud IAM | A cloud-credential flow, which is a larger feature than an adapter |
| Groq, Mistral, xAI, Cerebras | Rate limits are returned as headers on inference responses; no documented standalone "remaining quota" endpoint known | A documented account-usage endpoint |
| Windsurf, Kiro, Amp | No documented local quota data or public usage API known | A vendor API or stable local telemetry |
| Cline, Roo Code, Continue | Editor extensions that use the user's own API keys; any usage history is in extension storage that is not a stable interface | A stable export, or support through the underlying API provider |
| Aider, Goose | Use the user's own API keys; per-session cost is printed, no persistent quota | Support through the underlying API provider |
| Ollama, LM Studio | Local models: no quota, no cost | Nothing to monitor; out of scope |

## Compatibility by platform

| | Windows 11 x64 | macOS (Apple Silicon / Intel) | Linux |
|---|---|---|---|
| Core, CLI and parsers | Tested (unit, integration, CLI contract), locally and in CI | Tests pass in CI on both Apple Silicon and Intel runners | Tests pass in CI (Ubuntu 22.04) |
| Desktop overlay | Run and checked with fixture data | **Not tested** | **Not tested** |
| Claude Code login | Credentials file | Keychain via `/usr/bin/security`, **not tested** | Credentials file |
