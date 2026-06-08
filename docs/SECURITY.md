# Security Policy

## Reporting vulnerabilities

OpenBook is pre-release. Until a public security contact is established, report vulnerabilities privately to the maintainers through the repository owner contact channel. Do not disclose exploit details publicly before maintainers have acknowledged and triaged the report.

## Release-blocking security requirements

- Upstream Firefox source must be hash- and signature-verified before patching or building.
- OpenBook must not introduce unsolicited telemetry or first-run network egress.
- Proxy/VPN behavior must fail closed.
- Cryptographic erasure must invalidate keys rather than promise deletion or overwrite semantics.
- Lockout counters must be hardware-enforced where supported, and no-hardware fallback must be labeled weaker.
- Privileged files and native hosts must be root-owned and not user-writable in release packages.
- Destructive tests must use disposable VMs/containers and throwaway profiles only.
