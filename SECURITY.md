# Security Policy

## Supported releases

Security reports should be evaluated against the latest published Scarab release whenever practical.

The current release is available from the [latest Scarab release](https://github.com/FiendishDrWu/Scarab/releases/latest) page.

Older Scarab releases may not receive security fixes after a newer version has been published.

## Reporting a vulnerability

**Do not report a suspected security vulnerability in a public GitHub issue.**

Use GitHub's private vulnerability reporting for this repository:

1. Open the repository's **Security and quality** area.
2. Open **Advisories**.
3. Select **Report a vulnerability**.
4. Submit the report privately.

When possible, include:

- the affected Scarab version
- the SHA-256 of the affected `scarab.exe`
- a clear description of the suspected vulnerability
- the security impact you believe is possible
- reliable steps to reproduce or demonstrate the behavior
- relevant console output or other evidence
- whether the issue requires a specific MW5 installation, mod, pak, or crafted input

Do not include passwords, API keys, tokens, private signing material, or unrelated personal information.

Do not upload copyrighted MW5 game assets, complete commercial game pak files, or third-party mod files that you do not have permission to redistribute. Describe the minimum properties of an affected input or provide a lawful reproduction case instead.

## Scope

Security reports may concern the distributed Scarab executable, parsing or handling of untrusted game or mod data, output-path handling, release integrity or provenance, or another behavior that could affect Scarab users.

Scarab is closed-source. This public repository does not contain the Scarab application source code.

A public-source code review is therefore not required before submitting a good-faith security report. Reports based on observable executable behavior, release artifacts, or reproducible inputs are welcome.

## Disclosure

Please allow reasonable time to investigate a privately reported vulnerability before public disclosure.

Confirmed vulnerabilities may be handled through GitHub repository security advisories when appropriate.
