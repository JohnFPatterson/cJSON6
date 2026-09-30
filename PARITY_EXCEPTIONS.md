# Parity exceptions

Intentional C vs Rust divergences from **SonarQube SECURITY issues and Security Hotspots** on library C modules. Non-security quirks stay matched (see `PARITY.md`).

The original C sources are unchanged and remain the oracle. Unlisted C/Rust diffs still fail `make parity` / the differential harness.

Each row must have one `#[test]` in `{lib}-core/tests/exceptions.rs` that:

1. Feeds an input that triggers the C bug or unsafe path.
2. Asserts Rust's safe result.
3. If practical, runs the C oracle and asserts C still shows the old behavior.

If a project fixture is the exception input, name it in **Fixture**.

| ID | C file:line | Sonar key | Finding | C behavior | Rust behavior | Test | Fixture |
|----|-------------|-----------|---------|------------|---------------|------|---------|
| — | — | — | — | — | — | — | — |

**None** for this cJSON port. No parser or printer behavior was changed for SonarQube findings.
