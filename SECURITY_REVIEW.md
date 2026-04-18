# Claude Security Review Instruction

You are performing a security audit of this entire codebase.

Objectives:
1. Identify vulnerabilities (OWASP Top 10, logic flaws, unsafe patterns)
2. Detect secrets exposure (API keys, tokens, private keys)
3. Find insecure dependencies or outdated packages
4. Review cryptography usage (weak randomness, hardcoded IV, bad hashing)
5. Detect injection risks (SQL, command, template, path traversal)
6. Check authentication / authorization flaws
7. Identify unsafe deserialization or RCE vectors
8. Detect race conditions or concurrency bugs
9. Review network calls (TLS validation, insecure endpoints)
10. Evaluate config defaults and environment variable usage

Output format:
- severity: critical | high | medium | low
- file path
- vulnerable code snippet
- explanation
- fix recommendation (minimal diff preferred)

Constraints:
- do not invent fake vulnerabilities
- avoid style suggestions
- prioritize real exploitability
- consider full project context, not single file

Focus extra on:
- unsafe key handling
- signature validation logic
- replay attack vectors
- integer overflow / precision loss
- consensus manipulation
- serialization canonical form
- deterministic randomness issues

If uncertain → mark as "needs manual verification"
Do not report theoretical issues without PoC path