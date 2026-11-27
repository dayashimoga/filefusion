# Rule 05 — Security Standards

- **Untrusted Input Posture:** Treat every file, path, filename, archive member, XML/SVG tree, and metadata tag as potentially malicious (`FF-ARCH-001` §5).
- **Mandatory Bomb & Traversal Guards:** Reject `..`, leading `/` or `\`, drive letters, null bytes, and escaping symlinks/hardlinks. Enforce `ResourceLimits` (`100:1` max expansion ratio, `max_archive_depth = 3`, max uncompressed size, SVG anti-XXE).
- **Zero Shell Injection (`MOD-RULE-04`):** Invoke Tier 2 external binaries exclusively via `IsolatedProcessRunner` (`Vec<OsString>` arguments, sanitized environment, isolated private temp dir, process-group kill). Never construct `sh -c` or `cmd /c` shell strings.
- **Privacy & AI Consent:** Core tools work 100% offline. Never send private files, paths, or contents to external AI or cloud APIs without an explicit user Consent Gate (`FR-AI-002`).
- **Secret Hygiene:** Never hardcode, log, or commit API keys, tokens, passwords, or `.env` files.
