<p align="center">
  <img src="assets/scarab-logo.png" alt="Scarab" width="520">
</p>

<p align="center">
  <strong>Asset catalog generator for JJ's MechWarrior 5: Mercenaries Save Editor.</strong>
</p>

<p align="center">
  <a href="https://github.com/FiendishDrWu/Scarab/releases/latest">Latest release</a>
  ·
  <a href="https://github.com/FiendishDrWu/Scarab/issues">Report a problem</a>
  ·
  <a href="https://github.com/FiendishDrWu/Scarab/issues">Request a feature</a>
</p>

---

# Scarab

Scarab is a standalone Windows utility that generates asset catalogs for [JJ's MechWarrior 5: Mercenaries Save Editor](https://github.com/jonayetjubaer-cmyk/JJs-MW5-Merc-Save-Editor).

It reads a local MechWarrior 5: Mercenaries installation, discovers the base-game assets and mods enabled by the game, and generates catalogs describing available items, mechs, stock mech templates, and traits.

Scarab is designed primarily as a catalog-generation backend for JJ's editor. It may also be used independently.

## Download

STOP!

JJ's MW5 Save Editor already includes the correct version of Scarab. Running the incorrect Scarab version WILL break the editor.

DO NOT download Scarab from this repo unless you are absolutely certain of what you're doing.

Download the current release from the **[Latest Scarab release](https://github.com/FiendishDrWu/Scarab/releases/latest)**.

For the executable directly:

**[Download scarab.exe](https://github.com/FiendishDrWu/Scarab/releases/latest/download/scarab.exe)**

Scarab is distributed as a standalone Windows x64 executable. No installer is required.

The release contains three Scarab publication assets:

- `scarab.exe`
- `scarab.exe.sha256`
- `scarab.exe.virustotal.json`

Download `scarab.exe` to a location where it is allowed to create its output directory.

## Usage

Run Scarab from PowerShell, Command Prompt, or another program that launches executables.

Basic usage:

```powershell
.\scarab.exe --mw5-dir <MW5 game directory> --output <relative output directory>
```

Example:

```powershell
.\scarab.exe --mw5-dir "D:\MW5 Mercs\MW5Mercs" --output jj-catalog
```

Scarab discovers the base-game pak and Mods directory from the supplied MW5 game directory.

`--output` must be a relative path. The generated output directory is created relative to the directory containing `scarab.exe`.

### Command options

```text
--mw5-dir <MW5_DIR>
--output <OUTPUT>
--exclude-base-game
--exclude-mods
--exclude-mod <EXCLUDED_MOD_FOLDERS>
--catalog-format <CATALOG_FORMAT>
```

Catalog formats:

```text
python
json
```

The default catalog format is `python`.

### Examples

Generate the normal JJ-compatible Python catalogs from the base game and enabled mods:

```powershell
.\scarab.exe --mw5-dir "D:\MW5 Mercs\MW5Mercs" --output jj-catalog
```

Generate JSON catalogs using only the base game:

```powershell
.\scarab.exe --mw5-dir "D:\MW5 Mercs\MW5Mercs" --output base-json --exclude-mods --catalog-format json
```

Generate catalogs using enabled mods but excluding the base game:

```powershell
.\scarab.exe --mw5-dir "D:\MW5 Mercs\MW5Mercs" --output mods-only --exclude-base-game
```

Exclude one or more specific enabled mod folders:

```powershell
.\scarab.exe --mw5-dir "D:\MW5 Mercs\MW5Mercs" --output filtered --exclude-mod SomeModFolder --exclude-mod AnotherModFolder
```

## Generated files

With the default Python-compatible catalog format, Scarab generates:

```text
item_catalog.py
mech_catalog.py
trait_catalog.py
stock_templates.json.gz
catalog_build_report.json
```

With:

```text
--catalog-format json
```

the three catalog files become:

```text
item_catalog.json
mech_catalog.json
trait_catalog.json
```

The remaining files are unchanged:

```text
stock_templates.json.gz
catalog_build_report.json
```

Scarab always generates the complete catalog set.

## Base-game and mod discovery

By default, Scarab includes the MW5 base game and enabled mods.

For mods, Scarab reads MW5's `modlist.json` and includes only mods the game currently marks as enabled.

For each included mod, Scarab reads its `mod.json` load order and scans mod paks in merge order.

Specific enabled mods may be subtracted with:

```text
--exclude-mod <folder>
```

This option does not create a separate caller-defined mod list. It only excludes matching folders from the set already enabled by MW5.

Use:

```text
--exclude-mods
```

to ignore all mods.

Use:

```text
--exclude-base-game
```

to omit the base-game pak.

Scarab reads game and mod data only. It does not modify MW5 game files, mod files, `modlist.json`, or mod configuration.

## Verifying a Scarab release

Scarab is closed-source software. Because users cannot independently audit the source code, Scarab releases are published with multiple forms of integrity and provenance information.

### Authenticode signature

`scarab.exe` is Authenticode signed and RFC 3161 timestamped.

Windows users can inspect the signature from the executable's Properties dialog under **Digital Signatures**.

PowerShell may also be used:

```powershell
Get-AuthenticodeSignature .\scarab.exe |
    Format-List Status, StatusMessage, SignerCertificate, TimeStamperCertificate
```

With normal certificate trust and revocation checks available, the release executable should report a valid signature.

### SHA-256 checksum

Every release includes:

```text
scarab.exe.sha256
```

Calculate the downloaded executable's SHA-256 hash with:

```powershell
Get-FileHash .\scarab.exe -Algorithm SHA256
```

The resulting hash must exactly match the digest recorded in `scarab.exe.sha256` and on the GitHub Release.

### VirusTotal release manifest

Every release includes:

```text
scarab.exe.virustotal.json
```

This records the completed VirusTotal release-time analysis associated with the exact SHA-256 of the published executable.

Scarab's release process requires:

```text
0 malicious
0 suspicious
```

before publication.

The manifest also contains the exact VirusTotal report URL for the signed executable.

A VirusTotal result is not a guarantee that software is malware-free. It is published as additional release-time evidence tied to the same executable SHA-256 used throughout the release process.

### Immutable GitHub Release and attestation

Scarab releases are published as immutable GitHub Releases.

Before publication, the exact release assets are uploaded and their SHA-256 digests are validated. After publication, the release tag and assets are protected by GitHub's release immutability feature.

GitHub also provides a Sigstore release attestation containing the release identity and cryptographic digests of the published assets.

Together, the Authenticode signature, published SHA-256 checksum, VirusTotal release manifest, immutable GitHub Release, and GitHub release attestation provide multiple ways to verify that a downloaded executable matches the genuine published Scarab release.

## Closed-source status

Scarab is closed-source software.

This public repository is used for:

- official Scarab releases
- public documentation
- bug reports
- feature requests

The Scarab source code and private build repository are not published here.

GitHub automatically displays **Source code (zip)** and **Source code (tar.gz)** downloads for tagged releases. Those archives contain the contents of this public documentation and release repository. They are **not the Scarab application source code**.

## Reporting problems

Use the public repository's [Issues](https://github.com/FiendishDrWu/Scarab/issues) page to report Scarab problems.

When reporting a catalog-generation problem, include:

- Scarab version
- the command or options used
- the generated `catalog_build_report.json`
- a clear description of the unexpected result

Do not upload copyrighted MW5 game assets or complete commercial game pak files to an issue.

For problems involving a specific mod, identify the mod and provide enough information to reproduce the problem without redistributing the mod author's files unless you have permission to do so.

## Feature requests

Feature requests may also be submitted through [Issues](https://github.com/FiendishDrWu/Scarab/issues).

---

Scarab is an independent tool.
