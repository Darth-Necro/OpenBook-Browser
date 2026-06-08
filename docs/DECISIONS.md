# OpenBook Browser Decision Log

## ADR-0001 — Firefox source pin for Phase 0

- **Date:** 2026-06-08
- **Decision:** Pin Phase 0 to Firefox `145.0.2` source tarball from Mozilla release infrastructure.
- **Options considered:** Track `latest/`; pin a numbered stable release; use ESR.
- **Rationale:** A numbered stable release gives reproducible source URLs and immutable verification inputs. `latest/` is convenient but not deterministic. ESR is valuable for long-lived maintenance but the kickoff asks for Firefox stable, and Phase 0 must first prove the unmodified stable build pipeline.
- **Follow-up:** Revisit ESR versus rapid stable as an explicit release-channel decision before Phase 1.

## ADR-0002 — Patch application mechanism

- **Date:** 2026-06-08
- **Decision:** Use an ordered git patch series applied with `git am --3way` when the extracted upstream source is a git worktree, otherwise fall back to `patch -p1`.
- **Options considered:** `git format-patch` series; quilt; ad-hoc file copies.
- **Rationale:** Ordered git patches are deterministic, auditable, and rebaseable across upstream Firefox stable releases. Quilt remains possible later, but git patches are simpler for initial CI.

## ADR-0003 — CI provider skeleton

- **Date:** 2026-06-08
- **Decision:** Start with GitHub Actions workflow files and keep full Firefox builds manually gated.
- **Options considered:** GitHub Actions; GitLab CI; self-hosted-only build scripts.
- **Rationale:** GitHub Actions offers a straightforward Linux, Windows, and macOS matrix for proving orchestration. Full Firefox builds are resource-intensive, so CI exposes a manual gate while local/static checks run on every push.

## ADR-0004 — Phase 0 governance/funding posture

- **Date:** 2026-06-08
- **Decision:** Document a nonprofit/donations/grants posture as the default sustainability model until a formal governance decision is made.
- **Options considered:** Nonprofit/donations/grants; parent commercial sponsor; ad/search/telemetry monetization.
- **Rationale:** The security invariants forbid telemetry and unwanted data exposure. Ad/search/telemetry-driven funding conflicts with the project mission. A parent sponsor may be viable later, but the initial public posture should preserve independence and user trust.
