#!/usr/bin/env python3
"""
Cursor beforeSubmitPrompt hook.

Blocks a prompt before it reaches the model if the prompt text, or any file
attached to it, contains a plaintext (unencrypted) API key / credential or PII.

Input  (stdin):  {"prompt": "...", "attachments": [{"type": "file"|"rule", "file_path": "..."}], ...}
Output (stdout): {"continue": true}
             or  {"continue": false, "user_message": "..."}

hooks.json sets "failClosed": true, so if this script crashes or times out
the prompt is blocked rather than silently allowed through.
"""
import json
import math
import os
import re
import sys

MAX_FILE_BYTES = 1_000_000  # don't scan attachments larger than ~1 MB

# ---------------------------------------------------------------------------
# Secrets: well-known key formats
# ---------------------------------------------------------------------------
SECRET_PATTERNS = [
    ("AWS access key ID", re.compile(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b")),
    ("GitHub token", re.compile(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36,}\b|\bgithub_pat_[A-Za-z0-9_]{22,}")),
    ("Anthropic API key", re.compile(r"\bsk-ant-[A-Za-z0-9_\-]{20,}")),
    ("OpenAI API key", re.compile(r"\bsk-(?!ant-)(?:proj-)?[A-Za-z0-9_\-]{20,}")),
    ("Google API key", re.compile(r"\bAIza[0-9A-Za-z_\-]{35}\b")),
    ("Slack token", re.compile(r"\bxox[abprs]-[A-Za-z0-9\-]{10,}")),
    ("Stripe live key", re.compile(r"\b(?:sk|rk)_live_[A-Za-z0-9]{20,}")),
    ("Private key", re.compile(r"-----BEGIN (?:RSA |EC |DSA |OPENSSH |PGP )?PRIVATE KEY-----")),
    ("JSON Web Token", re.compile(r"\beyJ[A-Za-z0-9_\-]{10,}\.eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}")),
]

# Generic assignments, e.g. API_KEY="9f8a7c..." or db_password: Xk29!pQ...
GENERIC_ASSIGNMENT = re.compile(
    r"(?i)\b([a-z0-9_\-]*(?:api[_\-]?key|secret|token|passwd|password|client[_\-]?secret|access[_\-]?key)[a-z0-9_\-]*)"
    r"\s*[:=]\s*['\"]?([^\s'\",;]{12,})"
)

# Values that are NOT plaintext secrets: encrypted blobs, secret-manager or
# env-var references, and obvious placeholders.
ENCRYPTED_OR_REFERENCE = re.compile(
    r"^(?:ENC\[|vault:|kms:|arn:aws:secretsmanager|sops:|\$\{|\$[A-Z_]|process\.env|os\.environ|<|\{\{)",
    re.IGNORECASE,
)
PLACEHOLDER = re.compile(
    r"(?i)(your[_\-]?|example|placeholder|changeme|dummy|redacted|xxxx|\*\*\*\*|test[_\-]?key|fake)"
)

# ---------------------------------------------------------------------------
# PII
# ---------------------------------------------------------------------------
SSN = re.compile(r"\b(?!000|666|9\d\d)\d{3}-(?!00)\d{2}-(?!0000)\d{4}\b")
CARD_CANDIDATE = re.compile(r"\b(?:\d[ \-]?){12,18}\d\b")
EMAIL = re.compile(r"\b[A-Za-z0-9._%+\-]+@([A-Za-z0-9.\-]+\.[A-Za-z]{2,})\b")
US_PHONE = re.compile(r"(?<![\d\-])(?:\+?1[\s.\-]?)?\(?\d{3}\)?[\s.\-]\d{3}[\s.\-]\d{4}(?![\d\-])")

# Email domains that are fine to mention (docs, tests, bots). Add your own org here.
ALLOWED_EMAIL_DOMAINS = {
    "example.com", "example.org", "example.net", "test.com",
    "users.noreply.github.com", "cursoragent.com",
}


def shannon_entropy(s: str) -> float:
    counts = {c: s.count(c) for c in set(s)}
    return -sum((n / len(s)) * math.log2(n / len(s)) for n in counts.values()) if s else 0.0


def luhn_ok(digits: str) -> bool:
    total, parity = 0, len(digits) % 2
    for i, ch in enumerate(digits):
        d = int(ch)
        if i % 2 == parity:
            d *= 2
            if d > 9:
                d -= 9
        total += d
    return total % 10 == 0


def redact(value: str) -> str:
    """Never echo a full secret back into the UI or logs."""
    return value[:4] + "…" if len(value) > 4 else "…"


def scan(text: str) -> list[tuple[str, str]]:
    findings: list[tuple[str, str]] = []
    known_spans: list[tuple[int, int]] = []

    for label, pattern in SECRET_PATTERNS:
        for m in pattern.finditer(text):
            findings.append((label, redact(m.group(0))))
            known_spans.append(m.span())

    for m in GENERIC_ASSIGNMENT.finditer(text):
        value = m.group(2)
        start, end = m.span(2)
        if any(start < e and s < end for s, e in known_spans):
            continue  # already reported by a specific key pattern
        if ENCRYPTED_OR_REFERENCE.match(value) or PLACEHOLDER.search(value):
            continue
        if shannon_entropy(value) >= 3.5:  # real secrets look random; "hello_world_config" doesn't
            findings.append((f"Plaintext credential ({m.group(1)})", redact(value)))

    for m in SSN.finditer(text):
        findings.append(("Social Security number", "***-**-" + m.group(0)[-4:]))

    for m in CARD_CANDIDATE.finditer(text):
        digits = re.sub(r"\D", "", m.group(0))
        if 13 <= len(digits) <= 19 and luhn_ok(digits):
            findings.append(("Payment card number", "…" + digits[-4:]))

    for m in EMAIL.finditer(text):
        s_, e_ = m.span()
        if any(s_ < e and s < e_ for s, e in known_spans):
            continue  # e.g. https://ghp_xxx@github.com is a token, not an email
        if m.group(1).lower() not in ALLOWED_EMAIL_DOMAINS:
            findings.append(("Email address", redact(m.group(0))))

    for m in US_PHONE.finditer(text):
        findings.append(("Phone number", "…" + re.sub(r"\D", "", m.group(0))[-4:]))

    # de-duplicate, keep order
    return list(dict.fromkeys(findings))


def read_attachment(path: str) -> str | None:
    try:
        if not os.path.isfile(path) or os.path.getsize(path) > MAX_FILE_BYTES:
            return None
        with open(path, "r", encoding="utf-8", errors="ignore") as fh:
            return fh.read()
    except OSError:
        return None


def main() -> None:
    payload = json.load(sys.stdin)
    results: list[tuple[str, list[tuple[str, str]]]] = []

    prompt_findings = scan(payload.get("prompt") or "")
    if prompt_findings:
        results.append(("your prompt", prompt_findings))

    for att in payload.get("attachments") or []:
        path = att.get("file_path")
        content = read_attachment(path) if path else None
        if content:
            att_findings = scan(content)
            if att_findings:
                results.append((os.path.basename(path), att_findings))

    if not results:
        print(json.dumps({"continue": True}))
        return

    lines = ["Prompt blocked: it contains sensitive data that shouldn't be sent to a model."]
    for where, findings in results:
        for label, sample in findings:
            lines.append(f"  • {label} in {where} ({sample})")
    lines.append("Remove or redact these, or reference secrets via env vars / your secret manager, then resend.")

    print(json.dumps({"continue": False, "user_message": "\n".join(lines)}))


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # failClosed in hooks.json turns a crash into a block
        print(f"scan_prompt.py error: {exc}", file=sys.stderr)
        sys.exit(1)
