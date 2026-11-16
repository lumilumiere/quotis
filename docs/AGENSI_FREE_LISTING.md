# Quotis AI Usage Intelligence: FREE listing draft

Prepared 2026-11-16. Not submitted. Desktop and skill are separate artifacts.

**Price:** FREE ($0), no subscription or payment requirement for this skill.

**Summary:** Interpret a Quotis usage export: reported capacity, reset windows,
local token counts and qualified planning estimates.

This text-only skill helps an agent explain a schema-1 Quotis export. It distinguishes
provider allowances from local usage and estimated cost, reports age and missing coverage,
and does not assume that a reset replenished a stale reading. Projections require suitable
inputs and assume the measured pace continues.

Use a pasted or explicitly supplied export without installing the desktop app. The bundled
sample is an offline demonstration with historical synthetic figures, not current user data.
No data means no personal capacity estimate.

Optional CLI workflows require a separately obtained matching Quotis 2.0 CLI.
**Quotis 2.0 is under release validation and is not publicly release-ready.** Do not install
an unverified development executable merely to use this skill. No installer is in the ZIP.
Quotis 0.2.1 does not provide this CLI/export contract. Until a verified release is available,
use the bundled sample or a user-supplied compatible export.

Current Windows desktop installers contain the GUI only. They do not install the optional
CLI or add it to PATH; obtain a verified matching CLI separately when one is released.

Adapters exist for Claude Code, Codex, Copilot, Cursor, Gemini CLI, Qwen Code, opencode,
OpenRouter and DeepSeek. They are fixture-tested, **live-unverified in this release run**.
Four use undocumented endpoints. No agreement with current provider displays is claimed.
Unsupported services are not monitored.

Offline use reads only the supplied export. Local CLI use requires terminal permission.
`--live` may read local provider files/credentials and contact providers; obtain explicit
permission first. macOS Keychain may prompt. Permission denial stops further attempts.
Usage given to an agent becomes conversation content handled by that agent's provider.

**Tags:** ai-usage, quota, rate-limits, developer-productivity, planning.

**Compatibility:** SKILL.md plus text references. Codex review and local deterministic
checks validate structure and offline arithmetic. Actual Agensi MCP delivery and marketplace
review remain UNVERIFIED. No claim of testing every agent.

Local ignored workspace: `commercial/`. Expected archive:
`commercial/dist/quotis-ai-usage-intelligence-1.0.0.zip`. Do not assume availability in other
checkouts. Inventory/hash/results are recorded in RELEASE_READINESS_FINAL.md.

Agensi accepts a ZIP with SKILL.md frontmatter and references, prohibits binary installers,
and limits agent uploads to 100 files/5 MB. [Packaging](https://www.agensi.io/docs/packaging).
FREE listings require no payout method; account/profile and review remain necessary.
[Pricing](https://www.agensi.io/docs/pricing-your-skill),
[creator setup](https://www.agensi.io/docs/creator-quickstart).

Owner tasks: confirm identity/contact and licence/privacy terms, upload the reviewed ZIP
as FREE, retain all limitations, and complete platform review. No publication performed.

Publication gate remains **UNVERIFIED**: the package licence still says DRAFT, the local
privacy draft has unresolved identity/contact/date fields, and no creator session is
authorized. After owner edits, rebuild the archive and recheck its hash/content before
upload. Technical package PASS does not satisfy these decisions.
