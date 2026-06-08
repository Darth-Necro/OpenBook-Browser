# OpenBook Browser Threat Model v0

## Phase 0 scope

Phase 0 protects the build and source-integrity pipeline before any browser behavior changes are made.

## Assets

- Verified upstream Firefox source tarball.
- Ordered OpenBook patch series.
- Build scripts, mozconfigs, CI workflow, release artifacts, and signing inputs.
- Future privileged files: AutoConfig, enterprise policies, native messaging host binary, and host manifests.

## Invariants carried into all phases

- No unsolicited telemetry or first-run network egress in OpenBook builds.
- Source tarballs must be verified by hash and detached signature before patching.
- Patch application must fail hard on conflicts.
- Privileged config and native-host files must not be user-writable in release packages.
- Destructive lockout and filesystem work must run only in disposable harnesses.

## Phase 0 adversaries

| Adversary | Capability | Mitigation in Phase 0 |
|---|---|---|
| Network attacker | Modifies downloads in transit | HTTPS plus Mozilla SHA256 manifest and detached signature verification. |
| Malicious mirror or stale cache | Serves the wrong source tarball | Exact version pin and SHA256 entry matching the requested tarball path. |
| Patch drift | Causes silent partial patching | `apply-patches.sh` exits on the first failed patch. |
| CI misconfiguration | Skips verification or builds wrong target | Matrix workflow runs Phase 0 checks and exposes explicit build target selection. |

## Out of scope until later phases

- Runtime browser privacy hardening and first-run egress testing are Phase 1 deliverables.
- Cryptographic lockout, profile encryption, and erasure are Phase 2 deliverables and must wait for the disposable harness.
- Proxy/VPN fail-closed behavior is a Phase 3 deliverable.
- AI provider sandboxing and opt-in controls are Phase 4 deliverables.
