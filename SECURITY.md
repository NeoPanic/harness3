# Security policy

Harness³ is pre-1.0. Security fixes target the current `main` branch; older revisions do not have a separate maintenance commitment.

## Report a vulnerability privately

Use [GitHub private vulnerability reporting](https://github.com/NeoPanic/harness3/security/advisories/new) for suspected authorization bypasses, checkpoint corruption, credential exposure, or unsafe recovery behavior. Do not include an unpatched exploit or private trace in a public issue.

Include the affected revision, a minimal reproduction, impact, and any proposed mitigation. Use synthetic data and redact tokens, credentials, conversation content, and provider continuations. Maintainers will assess the report and coordinate a fix through the private advisory; no response-time SLA is promised.

## Integration boundaries

- Models and their tool arguments are untrusted input. Tools must validate arguments and enforce resource-level authority before effects.
- Registered tools and plugins are trusted in-process Rust code. The runtime does not sandbox them.
- Hosts own identities, credentials, storage access, outbound recipients, and exclusive session ownership during recovery.
- Tool-side idempotency or transactions are required for reliable external writes. Unknown outcomes must not be blindly retried.
- Trace events can contain model requests, tool arguments, and results. Hosts control redaction, access, retention, and rendering of those payloads.

Read the [runtime contracts](docs/runtime.md) for recovery, delivery, and policy semantics. For ordinary defects without security impact, use the public bug-report template.
