# Locating the cause of an anti-malware false positive — a native-app case study

[日本語ドキュメントはこちら。](#ja)

> **Finding.** Two independent machine-learning anti-malware engines flagged a clean Rust desktop application. **Neither reacted to anything resembling malicious behaviour**, and the change that took the binary from flagged to clean **required no change to executable code**: across that verdict boundary the compared `.text` sections are **byte-for-byte identical** (§4.1). Both engines reacted to the PE **resource section (`.rsrc`)** — the application's **icon** — along two *different* measurements: Trapmine to its **entropy** (PNG-compressed frames are incompressible), Microsoft to its **size** (a 256×256 uncompressed frame is 262 KB). Re-encoding the same picture and capping it at 64×64 produced **0/70 clean**.
>
> **Bounded claim.** Microsoft's score does include a code-level input: removing `CREATE_NO_WINDOW` from the child-process spawn re-flags the binary, measured with the resource section held **bit-identical** (rows 6 vs 7, §5.1). This document therefore does *not* assert that code features are absent from these models. It asserts what was measured — **the verdict boundary that the fix crosses was decided by a picture**, between two binaries whose executable sections are identical — and it notes that the one code input found is *also* not a behavioural property: it governs whether a console window blinks on startup.

**Scope:** point-in-time (2026-07-23/24, with a v2.6.8 re-inspection postscript on 2026-07-27 — §6.2), xoksa v2.6.7/v2.6.8, Windows x86_64, MSVC.
**Purpose:** a reproducible method for locating *which part of a binary* an ML detection is keyed to, so the next developer does not have to guess.
**Related:** [security-design.md §5–§6](./security-design.md), [security-assessment.md §C.4](./security-assessment.md).

---

## 1. Subject

Two Windows binaries, same workspace, same toolchain (Rust 1.96.0 / MSVC), same `release` profile (`strip = true`, `lto = true`, `codegen-units = 1`):

| | `xoksa.exe` (engine) | `xoksa-desktop.exe` (UI shell) |
| :--- | :--- | :--- |
| Role | CLI + loopback HTTP server | Tauri shell: spawns the engine as a child process, points an OS WebView at the loopback dashboard |
| Subsystem | console | windows (GUI) |
| Icon | none | yes |
| Child process | never | yes (`std::process::Command`, explicit argv, no shell) |
| `unsafe` | none (`#![forbid(unsafe_code)]`) | none |
| Packing / obfuscation | none | none |
| VirusTotal | **0/69 clean** | **2/70** |

The engine is the important half of this table: it is a **known-clean control from the same toolchain**, and §4 shows why that matters more than anything else.

**Observed detections on the desktop binary** — all machine-learning heuristics (`!ml`, `.ml.score`), not signature matches:

| Vendor | Verdict |
| :--- | :--- |
| Microsoft | `Trojan:Win32/Wacatac.B!ml`, later `Wacatac.C!ml` / `Program:Win32/Wacapew.C!ml` |
| Trapmine | `Malicious.moderate.ml.score` |

---

## 2. PE anatomy — where the two binaries differ

Per-section raw size and Shannon entropy (measurement script in §3):

```
xoksa.exe (clean, 0/69)                 xoksa-desktop.exe (2/70)
  .text    7,079,424   6.235              .text    7,138,816   6.211
  .rdata   5,618,176   6.156              .rdata   2,400,256   5.739
  .data        3,584   1.042              .data        2,560   1.634
  .pdata     253,952   6.523              .pdata     305,152   6.525
  .rsrc        2,048   4.176              .rsrc       51,200   7.928   <-- 25x, high entropy
  .reloc      99,840   5.456              .reloc      26,624   5.462
```

The code sections are near-identical in profile. The single structural outlier is `.rsrc` — the resource section, which on Windows holds the icon, the version block, and the manifest. The engine has no icon, so its `.rsrc` is 2 KB of version info and manifest. The desktop's is 51 KB, and at entropy 7.928 it is effectively incompressible — because a modern `.ico` stores its frames as **PNG**, i.e. already-compressed data.

That is the hypothesis the rest of this document tests, one variable at a time.

---

## 3. Method

Four rules. They are the transferable part of this case study.

1. **One variable per build.** One change, one binary, one scan. A build that changes two things teaches nothing about either.
2. **Change the build, not the finished binary.** Editing a built PE (deleting resources with a resource editor, patching bytes) produces a file that corresponds to no buildable source state — and can produce a PE that no longer loads. Every hypothesis must be expressible as a source or build-script change, or it is not a finding you can ship.
3. **Diff against a known-clean control from the same toolchain.** The question is never "what is wrong with my binary" but "**what differs from the one that already passes**". If you have no clean binary, build a minimal one with your toolchain and scan it first; that is your baseline.
4. **Measure, don't guess.** Per-section size and entropy, resource tree, version strings — recorded per candidate in a table.

**Tools** (all free): Python + [Pillow](https://python-pillow.org/) for icon re-encoding; `dumpbin` (VS Build Tools) for headers and resources; [`rcedit`](https://github.com/electron/rcedit) for post-build version strings; VirusTotal as the oracle; and this script, which is the whole diagnostic apparatus:

```python
# pe_entropy.py — per-section raw size + Shannon entropy of a PE file
import math, struct, sys

d = open(sys.argv[1], "rb").read()
pe = struct.unpack_from("<I", d, 0x3C)[0]
nsec = struct.unpack_from("<H", d, pe + 6)[0]
opt = struct.unpack_from("<H", d, pe + 20)[0]
tbl = pe + 24 + opt

def entropy(b):
    if not b:
        return 0.0
    c = [0] * 256
    for x in b:
        c[x] += 1
    n = len(b)
    return -sum((k / n) * math.log2(k / n) for k in c if k)

for i in range(nsec):
    o = tbl + i * 40
    name = d[o:o + 8].rstrip(b"\0").decode()
    vsize, _, rsize, roff = struct.unpack_from("<IIII", d, o + 8)
    print(f"{name:10} raw={rsize:>9,}  entropy={entropy(d[roff:roff + rsize]):.3f}")
```

**Local Defender is not a usable oracle.** `MpCmdRun.exe -Scan -ScanType 3 -File <path> -DisableRemediation` reported "no threats found" for *every* build in §5, including the ones VirusTotal's Microsoft engine flagged. Raising the cloud-block level to match requires disabling Tamper Protection. Use VirusTotal itself, and budget for its rescan interval.

---

## 4. Which parts of the binary were judged

Stated precisely, within what the experiments measured: **the verdict boundary that the shipping fix crosses is not a code boundary** — §4.1 shows the executable sections are byte-identical across it. That is not the same as claiming code features play no part in these models: one code-level input, `CREATE_NO_WINDOW`, demonstrably participates in Microsoft's score (row 4 of the table below, and §5). The mapping from source construct to the measured feature that moved a verdict:

| Source construct | What it produces in the PE | Measured property | Vendor effect |
| :--- | :--- | :--- | :--- |
| `icons/icon.ico` with PNG-compressed frames | `.rsrc` icon blob, incompressible | entropy **7.928** | **Trapmine flags** |
| the icon's 256×256 frame, uncompressed (BMP) | +262 KB in `.rsrc` | size **297 KB** | **Microsoft flags** |
| `bundle.publisher` / `bundle.copyright` (`tauri.conf.json`), `OriginalFilename` (`[package.metadata.tauri-winres]`), `FileDescription` (applied post-build with `rcedit`) | `RT_VERSION` strings | present / absent | absent → **Microsoft flags** |
| `cmd.creation_flags(0x0800_0000)` — `CREATE_NO_WINDOW` on the engine spawn | code | present / absent | absent → **Microsoft flags** |
| `std::process::Command` child spawn, GUI subsystem, WebView2 imports | code / headers | — | baseline signal; not independently variable here |

Three of the five are decoration: two are properties of a *picture*, one is free text any author can type. The fourth, `CREATE_NO_WINDOW`, is a real code change but a cosmetic one — it suppresses a console flash. Only the fifth relates to behaviour, and it is the one we could not vary without deleting the feature.

### 4.1 The code is identical across the verdict boundary

Per-section SHA-256 (first 16 hex) of row 3 — `c373b932…`, which Microsoft names **`Trojan:Win32/Wacatac.C!ml`** — against row 6 — `5d267218…`, which **70 engines call clean**:

```
section      size         c373b932 (FLAGGED)     5d267218 (CLEAN)
.text        7,138,816    550376fd5c23d703   =   550376fd5c23d703    identical
.data            2,560    59f7f93203bcc03f   =   59f7f93203bcc03f    identical
.pdata         305,152    d933a33e63160a45   =   d933a33e63160a45    identical
.reloc          26,624    467c9d8fe65f8c4b   =   467c9d8fe65f8c4b    identical
.rdata       2,397,184    57776e65601d2cd1   ≠   1182da8dcefaaa6c    same size (MSVC build non-determinism)
.rsrc      297,472 vs 32,768                                          the icon — the intended difference
```

**The executable code section is byte-for-byte identical between the two.** So are the writable data, exception-handling, and relocation sections. Whatever the "trojan" verdict was about, it was not about the code, because the code did not change.

### 4.2 What was *not* involved

Not one of the markers a malware heuristic is nominally looking for was present in any of the seven builds — and none of them varied, so none of them can explain a verdict that did:

- no packing or obfuscation (no high-entropy **code** section — `.text` sits at 6.2 throughout)
- no `unsafe` anywhere in the project's own code (compiler-enforced by `#![forbid(unsafe_code)]`)
- no process injection, no API hooking, no thread injection into other processes
- no driver load, no registry writes, no autostart entry, no persistence of any kind
- no self-modifying code, no dynamic import resolution (`LoadLibrary` / `GetProcAddress`), no `VirtualAlloc`-based staging
- no bundled crypto blob — Windows TLS goes through the OS SChannel via `native-tls`

The single genuinely behavioural trait — launching a sibling binary as a child process with an explicit argument vector and no shell — was present in **every** build, including the ones that scored **0/70**. It is therefore, by the experiment's own logic, not what any verdict was keyed to.

**The dependency tree is excluded by the same logic.** All six desktop builds (rows 1–6) link the *identical* dependency set, resolved from an unmodified `Cargo.lock` — including row 6, which scored 0/70. A malicious or compromised crate cannot explain a verdict that changed while the dependency set did not. Independently: SCA over the shipped artifacts (`cargo audit`, `cargo deny`, `osv-scanner`) reports **0 exploitable** advisories; the release binary is built with `cargo auditable`, so it carries its own dependency list for downstream verification; and a CycloneDX SBOM is published per release.

The practical consequence for a developer facing a detection: **the project's own code and its dependency set are not where to look first.** Measure the resource section.

### 4.3 Three ASCII characters

Packaging the app as a Windows installer produced a cleaner instance than any experiment above. Tauri's bundler records how the app was packaged by patching a marker string into the binary at bundle time:

```
offset 0x78CBF8 — three bytes, identical file size, identical section table
  BUNDLE_TYPE_VAR_UNK    5d267218…   0/70   clean
  BUNDLE_TYPE_VAR_NSS    7137ac47…   0/70   clean
  BUNDLE_TYPE_VAR_MSI    447d4039…   1/69   Trojan:Win32/Wacatac.B!ml
```

The three binaries are byte-identical except for those three characters. **`MSI` is flagged as a trojan; `UNK` and `NSS` are clean.** The marker is read only by `tauri::app::bundle_type()`, an informational API this application never calls (no updater, no plugins), so the three builds are behaviourally identical — not approximately, *exactly*.

A plausible reading is that `msiexec.exe` is a well-known living-off-the-land vector, so the token appears in a great deal of malicious training data. Whatever the cause, the feature deciding a trojan verdict is the literal text `MSI` in a string constant.

This is where the finding stopped being academic. Shipping an MSI meant shipping a binary Microsoft calls a trojan — not because of anything the installer does, but because Tauri writes those three characters. It is a packaging-tool defect rather than an application one — the evidence here (three binaries differing only at offset `0x78CBF8`, their hashes and VirusTotal results, and the suggested marker change) is what an upstream report to `tauri-apps/tauri` would carry, should one be filed. The local fix was to keep the unpatched binary in the package: build with `cargo build --release`, let Tauri generate the WiX sources, then re-run WiX `light` with the pristine binary in place, and **fail the build if the packaged payload does not hash-match what was built** ([`xoksa-desktop/build-msi.ps1`](../../xoksa-desktop/build-msi.ps1)). The resulting installer and both payloads scan clean.

**Build-system trap.** `tauri-build` embeds `RT_VERSION` via `tauri-winres`, reading `[package.metadata.tauri-winres]` from `Cargo.toml`, and then **force-sets `FileDescription = productName`**. Adding the `winresource` crate to `build.rs` to set it instead embeds a *second* `RT_VERSION` and fails to link ([tauri#10154](https://github.com/tauri-apps/tauri/issues/10154)). A descriptive `FileDescription` therefore cannot come from source and must be applied post-build with `rcedit`.

---

## 5. Experiment matrix

Every row is a real build, scanned on VirusTotal. Only the stated variable differs.

| # | Build (SHA-256, truncated) | Icon in `.rsrc` | `.rsrc` size | entropy | version metadata | Microsoft | Trapmine | Score |
| :-- | :--- | :--- | ---: | ---: | :---: | :--- | :--- | :--- |
| 0 | `a7d59038…` **engine (control)** | *(none)* | 2,048 B | 4.176 | yes | ✅ clean | ✅ clean | **0/69** |
| 1 | `61987ec4…` | PNG, 6 frames | 51,200 B | 7.928 | **no** | ❌ `Wacatac.B!ml` | ❌ flag | 2/70 |
| 2 | `7aa728dc…` | PNG, 6 frames | 51,200 B | 7.928 | yes | ✅ clean | ❌ flag | 1/70 |
| 3 | `c373b932…` | **BMP**, 6 frames | 297,472 B | 2.835 | yes | ❌ `Wacatac.C!ml` | ✅ clean | 1/70 |
| 4 | `6f90b411…` | PNG 6 frames **+ 300 KB inert padding** | ~351,000 B | 5.381 | yes | ❌ `Wacapew.C!ml` | ✅ clean | 1/69 |
| 5 | `6a837e97…` | PNG, 6 frames | 51,200 B | 7.928 | yes | ✅ clean | ❌ flag | 1/69 |
| 6 | **`5d267218…`** | **BMP, 4 frames (16/32/48/64)** | **32,768 B** | **3.982** | yes | ✅ clean | ✅ clean | **0/70** |
| 7 | `b80d49bb…` — **row 6 minus `CREATE_NO_WINDOW`** | BMP, 4 frames (identical `.rsrc`) | 32,768 B | 3.982 | yes | ❌ `Wacapew.C!ml` | ✅ clean | 1/70 |

Row 5 is row 2 rebuilt from scratch: the recipe reproduces, so the verdicts are properties of the configuration, not of one lucky binary. Row 7 is row 6 with exactly one code-level change — `CREATE_NO_WINDOW` removed from the engine spawn — and nothing else.

### 5.1 The two mirror experiments

Microsoft's verdict was moved twice, in **opposite halves of the binary**, each time with the other half held byte-identical:

| Pair | Held identical | Changed | Microsoft |
| :--- | :--- | :--- | :--- |
| row 3 → row 6 | **the code** — `.text` / `.data` / `.pdata` / `.reloc` byte-identical (§4.1) | `.rsrc` 297,472 B → 32,768 B (the icon) | flag → **clean** |
| row 6 → row 7 | **the resources** — `.rsrc` bit-identical (section hash `9fc9e30d…`, 32,768 B @ 3.982) | `CREATE_NO_WINDOW` removed (`.text` differs) | clean → **flag** |

Both halves therefore carry weight, and each was demonstrated at byte level rather than inferred. This is precisely why the claim in this document is bounded as it is: the model is not reading only resources, and it is not reading only code.

Note the **direction** of the second experiment. The build that *suppresses* a console window is the clean one; the build that lets the console appear is the flagged one. Whether the model keys on the `CREATE_NO_WINDOW` flag itself or on some incidental consequence of removing it cannot be determined from outside. What is established is that a single change carrying **no security-behavioural significance in either direction** — whether a black window blinks on startup — flips a verdict named after a trojan family.

**Evidentiary status of the two non-icon variables** — both are now full matrix rows, reproducible by hash:

| Variable removed | Result | Evidence |
| :--- | :--- | :--- |
| PE version metadata | **Microsoft flags** | row 1 vs row 2 |
| `CREATE_NO_WINDOW` on the engine spawn | **Microsoft flags** | row 6 vs row 7 (`.rsrc` bit-identical) |

---

## 6. The detection model these rows imply

- **Trapmine — `.rsrc` entropy.** Flags at 7.928; clean at 5.381, 4.176, 2.835. **Boundary bracketed between 5.381 (clean) and 7.928 (flag)** — not bisected, so no sharper figure is claimed.
- **Microsoft — `.rsrc` size**, as the dominant controllable feature, with **two further necessary conditions, both measured**: version metadata (rows 1 vs 2) and `CREATE_NO_WINDOW` on the engine spawn (rows 6 vs 7). Clean at 2,048 B and 51,200 B; flags at 297,472 B and ~351,000 B. **Boundary bracketed between 51,200 B (clean) and 297,472 B (flag)** — likewise not bisected. All three conditions must hold together: removing any one of them re-flags the binary.

Row 4 is the decisive experiment. It keeps the *identical PNG icon* of Microsoft-clean row 5 and adds only inert, low-entropy padding to `.rsrc`. Microsoft flips to flag. Therefore Microsoft was never reading the icon's format or its entropy — **it was reading the section's size**, and rows 3 and 4 flag for the same reason despite having opposite icon encodings.

### 6.1 Methodological pitfall — the one-axis error

Worth recording, because it cost a day and is easy to repeat. After rows 3–5 the conclusion drawn was: both vendors read the *same* metric (entropy) in *opposite* directions, their thresholds coincide, therefore a both-clean build is **impossible**.

That was wrong. The space had been plotted along **one axis** (entropy) when the deciding variables were **two** (entropy *and* size). An impossibility argument is void while any unmeasured axis remains — and the falsifying sample was already in the repository: the control binary (row 0) is **low-entropy and clean on both engines**, which the entropy-only model cannot explain. Comparing against it was the step that had been skipped. Once made, the both-clean region was obvious, and row 6 followed within two hours.

**Rule that generalises:** before concluding "not fixable", (a) list the axes you have *not* measured, and (b) look at a passing sample you already own.

### 6.2 Postscript (v2.6.8 re-inspection): the same recipe re-flagged, then cleared on reanalysis — reputation, not content

Three days later the project moved to v2.6.8 and the Windows artifacts were rebuilt and re-inspected. The desktop binary now differed from the 0/70 v2.6.7 build (`5d267218…`) in exactly two ways: the version string (`2.6.7` → `2.6.8`), and the recompiled code bytes that a version bump plus routine dependency updates produce. Every controllable feature was held: the icon file is **byte-identical**, `.rsrc` is again **32,768 B @ entropy 3.982**, the bundle marker is `UNK`, and all four version-metadata fields are present.

On first upload to VirusTotal the new desktop binary (`3abe6aef…`) scored **1/70 — Microsoft `Trojan:Win32/Wacatac.C!ml`**. Two controls placed the cause on the reputation axis, not the file:

1. **The old binary had not aged into a detection.** `5d267218…` (the v2.6.7 build), rescanned the *same day*, was still **0/70** — so Microsoft's model had not drifted against the recipe.
2. **Reanalysing the new binary — with no change to the file — cleared it to 0/70.** A verdict that flips on reanalysis of an unchanged file is, by definition, not a property of the file.

This is the **first-seen penalty**: a cloud ML backend returns a precautionary verdict on a hash it has never encountered and for which it has no prevalence or reputation signal, then withdraws it once the hash has been seen without anything adverse. It is the same phenomenon as §9's central claim, seen from the other side — the structural experiments (§§4–6) held the file constant and varied the bytes; here the bytes were constant and the *verdict* varied, which isolates reputation as a variable in its own right.

**Consequence for anyone shipping an unsigned native binary:** every release is a brand-new hash, so its first downloaders can meet this transient verdict before the cloud settles. It is reputation accruing, not a detection — but a user cannot tell the difference at the moment it appears. What short-circuits it is reputation the vendor already trusts: a code-signing certificate carries it immediately; a false-positive submission grants it per-hash; download volume and time earn it slowly. None of those change a byte of the program. It is worth re-stating plainly: **a first-scan flag on a new build, and a clean score after reanalysis, are the same file — the number moved because the file became familiar, not because it became safe.**

### 6.3 The floor — an empty `fn main() {}` GUI exe is flagged too (2026-07-31)

§6.2 isolated *reputation* by holding the file constant. This postscript isolates the opposite end — the **minimum a build must contain to be flagged** — by stripping the application away entirely and scanning what is left. Four builds from the same toolchain, scanned the same day:

| Build | Subsystem | WebView2 | What it does | VirusTotal |
| :--- | :--- | :--- | :--- | :--- |
| `xoksa.exe` engine (control) | console | no | the analysis server | **0/69 clean** |
| a Tauri shell | GUI | yes | opens a window | **1/71** — Microsoft `Wacatac.B!ml` |
| a winit shell | GUI | **no** | opens a hidden window, exits | **1/71** — Kaspersky `Trojan-PSW.Win32.Stealer.gen` |
| `fn main() {}` | GUI | no | **nothing at all** | **2/71** — Microsoft `Wacatac.B!ml` + SecureAge `Malicious` |

Three measured conclusions:

1. **Removing WebView2 did not clear the verdict — it moved it.** The Tauri build is flagged by Microsoft and clean on Kaspersky; the WebView2-free winit build is clean on Microsoft and flagged by Kaspersky. WebView2 is therefore not the trigger: removing it silences one vendor and wakes another.

2. **The floor is an empty program.** `fn main() {}` — no dependencies, no window, no network, no file / registry / credential access, no version metadata, no icon — compiled to a GUI-subsystem PE, is flagged by two engines. One gives it the **same** `Trojan:Win32/Wacatac.B!ml` verdict it gives the full desktop; the other calls a 100 KB do-nothing binary a password **stealer**. There is nothing inside to react to. The only thing left is the *shape*: an unsigned, unknown-hash, Rust/MSVC, GUI-subsystem executable.

3. **So the verdict is a property of that shape and its absent reputation, not of the application.** No code, dependency, icon, function name, or structural change can clear it across engines, because the empty baseline — which has none of them — is already flagged; changing the contents only redistributes *which* engine fires (conclusion 1). This is the conclusion of §9 reached from the clean side, now proven from the flagged side: the score is not about the program.

**Reproducible by upload (2026-07-31):** empty GUI `b683cba3…` (102,912 B, 2/71); winit GUI, no WebView2 `d85af141…` (419,840 B, 1/71 Kaspersky); Tauri GUI `1d0beddd…` (7,676,416 B, 1/71 Microsoft). `rustc 1.96.0`, `x86_64-pc-windows-msvc`, `release` (`strip` / `lto` / `codegen-units = 1`).

The empty build is the whole program:

```toml
# Cargo.toml
[package]
name = "probe-hello-gui"
version = "0.1.0"
edition = "2021"

[profile.release]
strip = true
lto = true
codegen-units = 1
```

```rust
// src/main.rs
#![windows_subsystem = "windows"]
fn main() {}
```

---

## 7. The fix

One file: `xoksa-desktop/icons/icon.ico`, re-encoded.

```python
from PIL import Image
im = Image.open("icons/128x128@2x.png").convert("RGBA")
im.save("icons/icon.ico", format="ICO",
        sizes=[(16, 16), (32, 32), (48, 48), (64, 64)],  # no 128/256 frame
        bitmap_format="bmp")                             # uncompressed => low entropy
```

Result: `.rsrc` 32,768 B @ entropy 3.982 → **0/70**. No source code, dependency, capability, or feature changed.

Two constraints must hold **simultaneously**, and they pull in opposite directions:

- **Uncompressed (BMP) frames** — keeps entropy low (Trapmine).
- **Small total** — a single 256×256 BMP frame is 262 KB and alone exceeds the size budget (Microsoft). Cap the largest frame at 64×64.

Also preserved, both independently confirmed to matter to Microsoft: the four version-metadata fields, and `CREATE_NO_WINDOW` on the child spawn.

**Cost of the fix:** the icon is sharp to 64 px; Windows upscales it in Explorer's extra-large view. Adding a 128×128 BMP frame would put `.rsrc` at ~98 KB — between the clean 51 KB and the flagged 297 KB samples, i.e. **untested territory** — so it must not be added without a fresh scan.

**Regression guard.** The most likely future regression is a contributor "improving" the icon back to a 256×256 PNG. The constraint belongs in a comment next to the icon and in the release checklist, not only in this document.

---

## 8. Playbook

For a native binary flagged by an ML engine:

1. **Do not start by changing program code.** Measure first; the trigger is frequently not code.
2. **Get a clean control** from the same toolchain — another of your binaries at 0, or a minimal build you make for the purpose — and diff the PE section table against it.
3. **Measure `.rsrc` size and entropy** (§3). A GUI app with a modern PNG-based `.ico` carries a large, incompressible blob that a console app does not; that is the most common single difference.
4. **Fill in version metadata** — CompanyName, LegalCopyright, OriginalFilename, a descriptive FileDescription. Absence is measurably negative. On Tauri, mind the `tauri-winres` trap in §4.
5. **One variable per build, changed in the build.**
6. **Keep the table.** hash × variables × verdict. The table is the deliverable; a passing build without one is luck you cannot reproduce.
7. **Rebuild the winning recipe from scratch and rescan** before believing it (row 5).
8. **File false-positive reports in parallel**, never as the plan — vendor turnaround is weeks, and an acquired vendor's contact may be unattended indefinitely.
9. **Record why the build is shaped the way it is**, or the fix will be undone by the next person with good intentions.

---

## 9. What this does and does not establish

- The two verdicts were **decided by properties of an embedded picture and of free-text metadata fields**. Neither is a property of program behaviour, and both are trivially adjustable by any author, benign or otherwise.
- Consequently, an ML verdict of this class is **not evidence about what a program does**, and should not be read as such — by users, by developers, or by anyone quoting an aggregate score.
- **The same conclusion holds in the other direction, and it is the one worth stating loudest.** The 0/70 here was not earned by making the program safer; it was earned by re-encoding a picture. Across the two-point swing from 2/70 to 0/70, not one property bearing on the program's actual safety changed — the code sections are identical (§4.1). **A clean aggregate score is therefore not evidence that a binary is safe.** It is not an audit, and it must not be presented as one. What can be said about this program's safety is said elsewhere, by different means: the 33-item shipping inspection, SCA, the penetration playbook, and the SOT design constraints — recorded in [security-assessment.md](./security-assessment.md), not on a scan page.
- The exposure is systematic for a specific, ordinary shape of software: an unsigned GUI application with a modern high-resolution icon. Nothing about that shape is unusual, which is why the failure mode recurs.
- This does **not** establish that ML detection is without value, that these vendors act in bad faith, or that any particular threshold is intentional. It establishes what two specific models keyed on, in one measurable case, at one point in time.

---

## 10. Limits

- **Point-in-time.** Vendor models change continuously; every verdict here is re-verifiable only by hash, as observed on 2026-07-23/24.
- **One product, one platform.** Windows x86_64, one Tauri application. The *method* generalises; the specific thresholds may not.
- **Thresholds are bracketed, not bisected.** Microsoft's `.rsrc` size boundary lies between 51,200 B (clean) and 297,472 B (flag); Trapmine's entropy boundary between 5.381 (clean) and 7.928 (flag).
- **The models are multi-feature.** What is identified here is the set of *controllable* levers, not the full feature set; other features were never varied.
- **VirusTotal's engine configurations are not the vendors' shipping products** — local Defender at default settings flagged none of these builds.

---

<a id="ja"></a>

# アンチマルウェア誤検知の原因特定 — ネイティブアプリの実測ケーススタディ

> **結論。** 独立した 2 つの機械学習アンチマルウェアが、クリーンな Rust デスクトップアプリを検知した。**どちらもマルウェアらしい挙動には反応していない。** そして flag から clean へ移した変更は、**実行コードの変更を必要としなかった**——その判定境界を跨いで、比較した `.text` セクションは**バイト単位で完全一致**している（§4.1）。両者が反応したのは PE の **リソースセクション（`.rsrc`）** ＝ アプリの **アイコン**であり、しかも *別々の* 測定量に対してだった：Trapmine は **エントロピー**（PNG 圧縮フレームは圧縮不能）、Microsoft は **サイズ**（256×256 の非圧縮フレームは 262KB）。同じ絵を再符号化して 64×64 で打ち止めた結果が **0/70 クリーン**である。
>
> **主張の範囲。** Microsoft のスコアにはコード由来の入力が含まれる——子プロセス起動の `CREATE_NO_WINDOW` を外すと再び flag する。これはリソースセクションを**ビット単位で固定したまま**実測した（行 6 対 行 7・§5.1）。したがって本稿は「これらのモデルがコード特徴を一切使っていない」とは主張しない。主張するのは測定した範囲、すなわち **今回の修正が跨いだ判定境界は絵で決まっていた**——実行セクションが同一である 2 本のバイナリの間で——という事実であり、加えて、**見つかった唯一のコード入力もまた挙動の性質ではない**（起動時にコンソール窓が一瞬光るかどうかを決めるだけである）ことを併記する。

**範囲：** 時点情報（2026-07-23/24。2026-07-27 の v2.6.8 再検査後日談を §6.2 に追記）、xoksa v2.6.7/v2.6.8、Windows x86_64、MSVC。
**目的：** ML 検知が **バイナリのどの部分** に紐づいているかを特定するための再現可能な手法を残すこと。次の開発者が推測に頼らずに済むように。
**関連：** [security-design.md §5–§6](./security-design.md)、[security-assessment.md §C.4](./security-assessment.md)。

---

## 1. 対象

同一ワークスペース・同一ツールチェイン（Rust 1.96.0 / MSVC）・同一 `release` プロファイル（`strip`・`lto`・`codegen-units = 1`）から出る Windows バイナリ 2 本。

| | `xoksa.exe`（エンジン） | `xoksa-desktop.exe`（UI シェル） |
| :--- | :--- | :--- |
| 役割 | CLI＋ループバック HTTP サーバ | Tauri シェル：エンジンを子プロセスで起動し OS WebView をループバックへ向ける |
| サブシステム | console | windows（GUI） |
| アイコン | なし | あり |
| 子プロセス | 起動しない | 起動する（`std::process::Command`・明示 argv・シェル非経由） |
| `unsafe` | なし（`#![forbid(unsafe_code)]`） | なし |
| パッキング／難読化 | なし | なし |
| VirusTotal | **0/69 クリーン** | **2/70** |

この表で最重要なのはエンジンの列である。**同一ツールチェインで clean が実測されている対照**であり、§4 が示すとおり、これが他の何よりも効く。

**デスクトップで観測された検知** — いずれも機械学習ヒューリスティック（`!ml`・`.ml.score`）であり、シグネチャ一致ではない：

| ベンダー | 判定 |
| :--- | :--- |
| Microsoft | `Trojan:Win32/Wacatac.B!ml`、後に `Wacatac.C!ml` / `Program:Win32/Wacapew.C!ml` |
| Trapmine | `Malicious.moderate.ml.score` |

---

## 2. PE 構造 — 2 本はどこが違うか

セクション別の raw サイズとシャノンエントロピー（計測スクリプトは §3）：

```
xoksa.exe（clean・0/69）                 xoksa-desktop.exe（2/70）
  .text    7,079,424   6.235              .text    7,138,816   6.211
  .rdata   5,618,176   6.156              .rdata   2,400,256   5.739
  .data        3,584   1.042              .data        2,560   1.634
  .pdata     253,952   6.523              .pdata     305,152   6.525
  .rsrc        2,048   4.176              .rsrc       51,200   7.928   <-- 25倍・高エントロピー
  .reloc      99,840   5.456              .reloc      26,624   5.462
```

コードセクションのプロファイルはほぼ同じ。構造上の唯一の外れ値が `.rsrc` — Windows でアイコン・バージョンブロック・マニフェストを保持するリソースセクションである。エンジンはアイコンを持たないので `.rsrc` は 2KB（バージョン情報とマニフェストのみ）。デスクトップは 51KB、エントロピー 7.928 ＝ 実質圧縮不能。**現代の `.ico` はフレームを PNG（＝圧縮済みデータ）で格納する**からである。

以降は、この仮説を 1 変数ずつ検証する記録である。

---

## 3. 手法

ルールは 4 つ。本ケーススタディで**転用可能な部分**はここである。

1. **1 ビルドにつき 1 変数。** 1 変更・1 バイナリ・1 スキャン。2 つ変えたビルドはどちらについても何も教えない。
2. **完成品ではなくビルドを変える。** ビルド済み PE をリソースエディタで削る・バイト列を当てる等の後加工は、ビルド可能なソース状態に対応しないファイルを生む（ロード不能な PE になることもある）。仮説はソースまたはビルドスクリプトの変更として表現できねばならない。さもなくば出荷できる知見にならない。
3. **同一ツールチェインの clean な対照と差分する。** 問うべきは「自分のバイナリの何が悪いか」ではなく「**すでに通っている方と何が違うか**」。clean な現物が無ければ、同じツールチェインで最小構成をビルドして先にスキャンする。それが基準線になる。
4. **推測せず測る。** セクション別サイズとエントロピー、リソースツリー、バージョン文字列 — 候補ごとに表へ記録する。

**ツール**（すべて無償）：アイコン再符号化に Python＋[Pillow](https://python-pillow.org/)、ヘッダとリソース確認に `dumpbin`（VS Build Tools）、ビルド後のバージョン文字列に [`rcedit`](https://github.com/electron/rcedit)、判定装置として VirusTotal、そして診断装置の全体である以下のスクリプト（全文は英語節 §3）。

**ローカルの Defender は判定装置として使えない。** `MpCmdRun.exe -Scan -ScanType 3 -File <path> -DisableRemediation` は §5 の**全ビルド**について「脅威なし」を返した（VirusTotal 上の Microsoft エンジンが flag したものを含む）。クラウドブロックレベルを引き上げるには改ざん防止の解除が要る。判定は VirusTotal 自体で行い、再スキャン間隔を見込んで計画すること。

---

## 4. バイナリのどの部分が判定対象になったか

測定した範囲で正確に述べる：**今回の出荷修正が跨いだ判定境界は、コードの境界ではない**——§4.1 のとおり、その境界を跨いでも実行セクションはバイト単位で同一である。これは「これらのモデルがコード特徴を一切使っていない」という主張とは別である：コード由来の入力が 1 つ、`CREATE_NO_WINDOW` が Microsoft のスコアに関与することは実証されている（下表 4 行目・§5）。判定を動かした測定量と、それを生むソース構成要素の対応：

| ソース構成要素 | PE 上で生成するもの | 測定される性質 | ベンダーへの効果 |
| :--- | :--- | :--- | :--- |
| PNG 圧縮フレームを持つ `icons/icon.ico` | `.rsrc` のアイコン塊（圧縮不能） | エントロピー **7.928** | **Trapmine が flag** |
| そのアイコンの 256×256 フレーム（非圧縮 BMP） | `.rsrc` に +262KB | サイズ **297KB** | **Microsoft が flag** |
| `bundle.publisher`／`bundle.copyright`（`tauri.conf.json`）、`OriginalFilename`（`[package.metadata.tauri-winres]`）、`FileDescription`（ビルド後に `rcedit`） | `RT_VERSION` の文字列 | 有無 | 無 → **Microsoft が flag** |
| `cmd.creation_flags(0x0800_0000)` ＝ エンジン起動時の `CREATE_NO_WINDOW` | コード | 有無 | 無 → **Microsoft が flag** |
| `std::process::Command` による子プロセス起動・GUI サブシステム・WebView2 インポート | コード／ヘッダ | — | ベースラインのシグナル（本件では独立に変化させられない） |

5 つのうち 3 つは装飾である：2 つは **絵** の性質、1 つは誰でも任意に打てる自由記述欄。4 つ目の `CREATE_NO_WINDOW` は実際のコード変更だが、コンソール窓の一瞬の表示を抑えるだけの見た目の話。挙動に関わるのは 5 つ目だけで、それは機能を削除せずには変化させられなかった。

### 4.1 判定の境界を跨いでもコードは同一である

行 3（`c373b932…` ＝ Microsoft が **`Trojan:Win32/Wacatac.C!ml`** と名指ししたもの）と、行 6（`5d267218…` ＝ **70 エンジンが clean** としたもの）の、セクション別 SHA-256（先頭 16 桁）：

```
セクション    サイズ        c373b932（FLAG）        5d267218（CLEAN）
.text        7,138,816    550376fd5c23d703   =   550376fd5c23d703    完全一致
.data            2,560    59f7f93203bcc03f   =   59f7f93203bcc03f    完全一致
.pdata         305,152    d933a33e63160a45   =   d933a33e63160a45    完全一致
.reloc          26,624    467c9d8fe65f8c4b   =   467c9d8fe65f8c4b    完全一致
.rdata       2,397,184    57776e65601d2cd1   ≠   1182da8dcefaaa6c    同サイズ（MSVC ビルドの非決定性）
.rsrc      297,472 / 32,768                                           アイコン ＝ 意図した唯一の差
```

**実行コードセクションは両者でバイト単位に完全一致している。** 書込データ・例外処理・再配置の各セクションも同様。この「トロイの木馬」判定が何についてのものであれ、**コードについてではない**。コードは変わっていないのだから。

### 4.2 何が関与していなかったか

マルウェアのヒューリスティックが本来探すはずのマーカーは、7 ビルドのいずれにも存在せず、かつ**一つも変化していない**。変化していない要因は、変化した判定を説明できない。

- パッキング・難読化なし（**コード**セクションは高エントロピーでない — `.text` は全ビルドで 6.2 前後）
- プロジェクト自身のコードに `unsafe` なし（`#![forbid(unsafe_code)]` によりコンパイラ強制）
- プロセスインジェクションなし・API フックなし・他プロセスへのスレッド注入なし
- ドライバロードなし・レジストリ書込なし・autostart 登録なし・**自己永続化なし**
- 自己書換コードなし・動的インポート解決なし（`LoadLibrary` / `GetProcAddress`）・`VirtualAlloc` によるステージングなし
- 暗号ライブラリの同梱なし（Windows の TLS は `native-tls` 経由で OS の SChannel を使用）

唯一の実質的な挙動——隣に同梱したバイナリを、明示的な引数配列で、シェルを介さずに子プロセスとして起動する——は、**全ビルドに存在した**。**0/70 を取ったビルドにも、である。** したがって実験自身の論理により、それはどの判定の根拠でもない。

**依存ツリーも同じ論理で排除される。** デスクトップの 6 ビルド（行 1〜6）は、変更していない `Cargo.lock` から解決した **同一の依存セット**をリンクしている——0/70 を取った行 6 を含めて、である。**変化していない依存セットは、変化した判定を説明できない。** 独立した裏付けとして：出荷現物に対する SCA（`cargo audit`・`cargo deny`・`osv-scanner`）は **exploitable 0**、リリースビルドは `cargo auditable` で依存リストをバイナリ自身に内蔵（下流で検証可能）、CycloneDX SBOM をリリース毎に公開。

検知に直面した開発者にとっての実務的な帰結：**自分のコードと依存クレートは、最初に見る場所ではない。** リソースセクションを測ること。

### 4.3 ASCII 3 文字

Windows インストーラとして梱包する作業で、上記のどの実験よりも純粋な事例が出た。Tauri のバンドラは、梱包形式を記録するためにマーカー文字列をバイナリへ後付けパッチする。

```
offset 0x78CBF8 — 3 バイト。ファイルサイズもセクション表も完全に同一
  BUNDLE_TYPE_VAR_UNK    5d267218…   0/70   clean
  BUNDLE_TYPE_VAR_NSS    7137ac47…   0/70   clean
  BUNDLE_TYPE_VAR_MSI    447d4039…   1/69   Trojan:Win32/Wacatac.B!ml
```

3 本のバイナリはこの 3 文字を除いてバイト単位で同一である。**`MSI` はトロイの木馬と判定され、`UNK` と `NSS` はクリーン。** このマーカーを読むのは `tauri::app::bundle_type()` だけで、本アプリはこの情報取得 API を一度も呼ばない（updater もプラグインも未使用）。よって 3 本は挙動の上でも同一である——近似的にではなく、**厳密に**。

`msiexec.exe` が LOLBin として悪用される定番であるため、このトークンが悪性の学習データに大量に現れる、という読みは成り立つ。原因が何であれ、**トロイの木馬判定を決めている特徴は、文字列定数の中の `MSI` という文字そのもの**である。

ここで所見は机上の話でなくなった。MSI で配布するということは、Microsoft がトロイの木馬と呼ぶバイナリを配ることを意味した——インストーラの動作のせいではなく、Tauri がその 3 文字を書き込むからである。これはアプリ側ではなく**梱包ツール側の欠陥**であり、ここに記録した証拠（オフセット `0x78CBF8` の 3 バイトだけが違う 3 本のバイナリ、そのハッシュと VirusTotal 結果、マーカー値の変更提案）は、`tauri-apps/tauri` へ報告する場合にそのまま材料となる。ローカルでの対処は、パッチされていないバイナリを梱包に残すこと：`cargo build --release` でビルドし、Tauri には WiX ソースだけ生成させ、無傷のバイナリを置いた状態で WiX `light` を再実行する。そして**梱包された payload がビルド物とハッシュ一致しなければビルドを失敗させる**（[`xoksa-desktop/build-msi.ps1`](../../xoksa-desktop/build-msi.ps1)）。結果として、インストーラと同梱物 2 本のすべてがクリーンになった。

**ビルドシステムの罠。** `tauri-build` は `tauri-winres` 経由で `RT_VERSION` を埋め込み、`Cargo.toml` の `[package.metadata.tauri-winres]` を読んだうえで **`FileDescription = productName` を強制上書きする**。代わりに `build.rs` へ `winresource` クレートを足すと `RT_VERSION` が **二重** に埋まりリンクエラーになる（[tauri#10154](https://github.com/tauri-apps/tauri/issues/10154)）。したがって説明的な `FileDescription` はソースからは設定できず、ビルド後に `rcedit` で当てるしかない。

---

## 5. 実験行列

各行は実際のビルドで、VirusTotal で実測。明記した変数以外に差はない。

| # | ビルド（SHA-256 先頭） | `.rsrc` 内のアイコン | `.rsrc` サイズ | entropy | バージョンメタデータ | Microsoft | Trapmine | スコア |
| :-- | :--- | :--- | ---: | ---: | :---: | :--- | :--- | :--- |
| 0 | `a7d59038…` **エンジン（対照）** | *(なし)* | 2,048 B | 4.176 | あり | ✅ clean | ✅ clean | **0/69** |
| 1 | `61987ec4…` | PNG・6 フレーム | 51,200 B | 7.928 | **なし** | ❌ `Wacatac.B!ml` | ❌ flag | 2/70 |
| 2 | `7aa728dc…` | PNG・6 フレーム | 51,200 B | 7.928 | あり | ✅ clean | ❌ flag | 1/70 |
| 3 | `c373b932…` | **BMP**・6 フレーム | 297,472 B | 2.835 | あり | ❌ `Wacatac.C!ml` | ✅ clean | 1/70 |
| 4 | `6f90b411…` | PNG 6 フレーム **＋ 不活性な詰め物 300KB** | 約 351,000 B | 5.381 | あり | ❌ `Wacapew.C!ml` | ✅ clean | 1/69 |
| 5 | `6a837e97…` | PNG・6 フレーム | 51,200 B | 7.928 | あり | ✅ clean | ❌ flag | 1/69 |
| 6 | **`5d267218…`** | **BMP・4 フレーム（16/32/48/64）** | **32,768 B** | **3.982** | あり | ✅ clean | ✅ clean | **0/70** |
| 7 | `b80d49bb…` — **行 6 から `CREATE_NO_WINDOW` を外しただけ** | BMP・4 フレーム（`.rsrc` は同一） | 32,768 B | 3.982 | あり | ❌ `Wacapew.C!ml` | ✅ clean | 1/70 |

行 5 は行 2 をゼロから再ビルドしたもの。**レシピが再現する**＝判定は 1 本のバイナリの偶然ではなく構成の性質である、ことの確認。行 7 は行 6 に対しコード側の変更をちょうど 1 つ——engine 起動から `CREATE_NO_WINDOW` を外す——だけ加えたもので、他は一切同一。

### 5.1 2 つの鏡像実験

Microsoft の判定は 2 度動かされている。**バイナリの正反対の半分**で、それぞれ**もう一方をバイト単位で固定したまま**：

| 組 | 固定した側 | 変えた側 | Microsoft |
| :--- | :--- | :--- | :--- |
| 行 3 → 行 6 | **コード** — `.text`／`.data`／`.pdata`／`.reloc` がバイト単位で一致（§4.1） | `.rsrc` 297,472 B → 32,768 B（アイコン） | flag → **clean** |
| 行 6 → 行 7 | **リソース** — `.rsrc` がビット単位で一致（セクションハッシュ `9fc9e30d…`・32,768 B @ 3.982） | `CREATE_NO_WINDOW` を除去（`.text` が差分） | clean → **flag** |

したがって**両方の半分が重みを持つ**。しかもいずれも推論ではなくバイト単位で実証されている。本稿の主張を限定しているのはこのためである：モデルはリソースだけを読んでいるのでもなく、コードだけを読んでいるのでもない。

2 つ目の実験は**向き**に注目してほしい。**コンソール窓を「抑制する」方のビルドが clean であり、コンソールが出る方が flag される。** モデルが `CREATE_NO_WINDOW` フラグ自体を見ているのか、除去に伴う副次的な差を見ているのかは、外部からは判別できない。確立できるのは、**どちらの向きにもセキュリティ上の意味を持たない変更**——起動時に黒い窓が一瞬光るかどうか——が、**トロイの木馬ファミリー名を冠した判定を反転させる**という事実である。

**アイコン以外の 2 変数の証拠水準** — いずれも**ハッシュで再現できる正式な行**になった：

| 外した変数 | 結果 | 根拠 |
| :--- | :--- | :--- |
| PE バージョンメタデータ | **Microsoft が flag** | 行 1 対 行 2 |
| engine 起動時の `CREATE_NO_WINDOW` | **Microsoft が flag** | 行 6 対 行 7（`.rsrc` ビット単位で同一） |

---

## 6. 行列から導かれる検知モデル

- **Trapmine — `.rsrc` のエントロピー。** 7.928 で flag、5.381・4.176・2.835 は clean。**境界は 5.381（clean）と 7.928（flag）の間に挟み込まれる** — 二分探索していないので、それ以上に鋭い数値は主張しない。
- **Microsoft — `.rsrc` のサイズ**（制御可能な支配的特徴として）。加えて**実測済の必要条件が 2 つ**：バージョンメタデータ（行 1 対 行 2）と、engine 起動時の `CREATE_NO_WINDOW`（行 6 対 行 7）。2,048 B・51,200 B は clean、297,472 B・約 351,000 B は flag。**境界は 51,200 B（clean）と 297,472 B（flag）の間に挟み込まれる** — 同じく二分探索していない。**3 条件は同時に成立する必要があり、どれか 1 つを外せば再び flag する。**

決定的なのは行 4 である。Microsoft clean だった行 5 と **同一の PNG アイコン** を保持したまま、不活性・低エントロピーの詰め物を `.rsrc` に足しただけで、Microsoft が flag に転じた。よって Microsoft はアイコンの形式もエントロピーも読んでいない — **読んでいたのはセクションのサイズ**であり、アイコンの符号化が正反対である行 3 と行 4 が同じ理由で flag する。

### 6.1 手法上の落とし穴 — 一軸誤り

丸一日を要し、かつ再発しやすいので記録する。行 3〜5 の時点で出した結論は「2 社は *同じ* 指標（エントロピー）を *逆向き* に読んでおり、閾値が重なるので両方 clean のビルドは **不可能**」だった。

**これは誤り。** 決定変数が **2 つ**（エントロピー **と** サイズ）であるところを、**一軸**（エントロピー）で空間を張っていた。未測定の軸が残っている限り、不可能性の議論は成立しない。しかも反証標本は最初からリポジトリの中にあった：対照バイナリ（行 0）は **低エントロピーで両社 clean** であり、エントロピー単独モデルでは説明できない。飛ばしていたのは「それと比較する」工程だった。比較した時点で両方 clean の領域は自明になり、2 時間後に行 6 に到達した。

**一般化できるルール：** 「直せない」と結論する前に、(a) **測っていない軸**を列挙し、(b) **すでに手元にある合格標本**を見る。

### 6.2 後日談（v2.6.8 再検査）：同じレシピが再ビルドで再び flag、Reanalyze で解消 — 中身ではなくレピュテーション

3 日後、プロジェクトは v2.6.8 へ進み、Windows 現物を再ビルドして再検査した。desktop 現物は、0/70 だった v2.6.7 ビルド（`5d267218…`）と**ちょうど 2 点だけ**異なる：版数文字列（`2.6.7`→`2.6.8`）と、版数更新＋通常の依存更新が生む再コンパイル済みコードバイト。制御可能な特徴はすべて保たれている——アイコンファイルは**バイト単位で同一**、`.rsrc` は再び **32,768 B @ entropy 3.982**、バンドルマーカーは `UNK`、バージョンメタデータ 4 項目も完備。

VirusTotal への初回アップロードで、この新しい desktop 現物（`3abe6aef…`）は **1/70 — Microsoft `Trojan:Win32/Wacatac.C!ml`**。2 つの対照が、原因をファイルではなく**レピュテーション軸**に置いた：

1. **旧現物は検知へと「経年変化」していない。** `5d267218…`（v2.6.7 ビルド）を**同じ日**に再スキャンしても **0/70** のまま——Microsoft のモデルがこのレシピに対してドリフトしたわけではない。
2. **新現物を——ファイルを一切変えずに——Reanalyze すると 0/70 に解消した。** 変えていないファイルの判定が再解析で覆るなら、それは定義上ファイルの性質ではない。

これが**初見ペナルティ（first-seen penalty）**である：クラウド ML は、一度も見たことがなく普及度もレピュテーションのシグナルも無いハッシュに対して予防的判定を返し、悪性の兆候なくそのハッシュが観測された時点でそれを撤回する。§9 の中心的主張を裏側から見たものだ——構造実験（§4〜6）はファイルを固定してバイトを変えた。ここではバイトが固定でありながら**判定**が変わった。これはレピュテーションを独立した変数として分離している。

**未署名ネイティブバイナリを配布する者にとっての帰結：** リリースは毎回まったく新しいハッシュなので、その最初のダウンロード者は、クラウドが落ち着く前にこの一過性の判定に出会いうる。それは検知ではなくレピュテーションの蓄積途中だが——出た瞬間、利用者にはその区別がつかない。これを短絡させるのは、ベンダーが既に信頼しているレピュテーションである：コード署名証明書は即座にそれを運び、誤検知申請はハッシュ単位で付与し、ダウンロード数と時間はゆっくり獲得する。いずれもプログラムのバイトを 1 つも変えない。平易に言い直す価値がある——**新規ビルドの初回スキャンの flag と、Reanalyze 後の clean は、同じファイルである。数字が動いたのは、ファイルが安全になったからではなく、見慣れられたからだ。**

### 6.3 底 — 空の `fn main() {}` の GUI exe すら flag される（2026-07-31）

§6.2 はファイルを固定して*レピュテーション*を分離した。本追記は逆側——**flag されるためにビルドが最低限含むべきもの**——を、アプリ本体を丸ごと剥ぎ取って残りをスキャンすることで分離する。同一ツールチェインの4ビルドを同日に実測：

| ビルド | サブシステム | WebView2 | 何をするか | VirusTotal |
| :--- | :--- | :--- | :--- | :--- |
| `xoksa.exe` エンジン（対照） | console | なし | 分析サーバ | **0/69 clean** |
| Tauri シェル | GUI | あり | ウィンドウを開く | **1/71** — Microsoft `Wacatac.B!ml` |
| winit シェル | GUI | **なし** | 隠しウィンドウを開いて即終了 | **1/71** — Kaspersky `Trojan-PSW.Win32.Stealer.gen` |
| `fn main() {}` | GUI | なし | **何もしない** | **2/71** — Microsoft `Wacatac.B!ml` ＋ SecureAge `Malicious` |

実測された結論は3つ：

1. **WebView2 を外しても判定は消えず——移動した。** Tauri ビルドは Microsoft が flag／Kaspersky は clean、WebView2 を外した winit ビルドは Microsoft が clean／Kaspersky が flag。＝WebView2 は犯人ではない。外すと一方の engine が黙り、別の engine が鳴る。

2. **底は「空のプログラム」。** `fn main() {}`——依存ゼロ・ウィンドウ無し・ネットワーク無し・ファイル/レジストリ/資格情報アクセス無し・バージョンメタデータ無し・アイコン無し——を GUI サブシステムでコンパイルしただけの PE が、2 engine に flag される。一方は完成品のデスクトップと**同じ** `Trojan:Win32/Wacatac.B!ml`、もう一方は 100KB の何もしないバイナリを**パスワード窃取（stealer）**と呼ぶ。反応する中身は無い。残るのは*素性*だけ：未署名・無名ハッシュ・Rust/MSVC・GUI サブシステムの実行ファイル。

3. **＝判定は、その素性と欠落したレピュテーションの性質であって、アプリの性質ではない。** コード・依存・アイコン・関数名・構造をどう変えても、全 engine を横断して消すことはできない。空のベースライン（それらを一切持たない）が既に flag されているからだ。中身を変えても*どの* engine が鳴るかが入れ替わるだけ（結論1）。これは §9 が clean 側から得た結論を flag 側から実証したもの：スコアはプログラムについての情報ではない。

**アップロードで再現可能（2026-07-31）：** 空 GUI `b683cba3…`（102,912 B・2/71）／winit GUI・WebView2 なし `d85af141…`（419,840 B・1/71 Kaspersky）／Tauri GUI `1d0beddd…`（7,676,416 B・1/71 Microsoft）。`rustc 1.96.0`・`x86_64-pc-windows-msvc`・`release`（`strip` / `lto` / `codegen-units = 1`）。

空ビルドはこれで全部：

```toml
# Cargo.toml
[package]
name = "probe-hello-gui"
version = "0.1.0"
edition = "2021"

[profile.release]
strip = true
lto = true
codegen-units = 1
```

```rust
// src/main.rs
#![windows_subsystem = "windows"]
fn main() {}
```

---

## 7. 修正内容

1 ファイル：`xoksa-desktop/icons/icon.ico` の再符号化のみ。

```python
from PIL import Image
im = Image.open("icons/128x128@2x.png").convert("RGBA")
im.save("icons/icon.ico", format="ICO",
        sizes=[(16, 16), (32, 32), (48, 48), (64, 64)],  # 128/256 フレームを持たない
        bitmap_format="bmp")                             # 非圧縮＝低エントロピー
```

結果：`.rsrc` 32,768 B @ entropy 3.982 → **0/70**。ソースコード・依存・capability・機能はいずれも不変。

**同時に**満たすべき制約が 2 つあり、互いに逆方向へ引く：

- **非圧縮（BMP）フレーム** — エントロピーを低く保つ（Trapmine）。
- **合計を小さく** — 256×256 の BMP フレームは単体で 262KB あり、それだけでサイズ予算を超える（Microsoft）。最大フレームを 64×64 で打ち止める。

併せて維持：バージョンメタデータ 4 項目と、子プロセス起動の `CREATE_NO_WINDOW`。いずれも Microsoft に効くことを個別に確認済み。

**修正の代償：** アイコンは 64px まで鮮明で、Explorer の特大アイコン表示では Windows が拡大する。128×128 の BMP フレームを足すと `.rsrc` は約 98KB となり、clean 実測 51KB と flag 実測 297KB の**未検証帯**に入る。**再スキャンなしに追加してはならない。**

**退行防止。** 最も起こりやすい将来の退行は、貢献者がアイコンを 256×256 PNG に「改善」することである。この制約は本ドキュメントだけでなく、**アイコンの隣のコメントとリリースチェックリストに置く**べきである。

---

## 8. 手順書

ML エンジンに検知されたネイティブバイナリに対して：

1. **プログラムコードの書き換えから始めない。** まず測る。引き金がコードでないことは頻繁にある。
2. **同一ツールチェインの clean な対照を用意する** — 0 が出ている自分の別バイナリ、または目的のために作る最小ビルド — そして PE のセクション表を差分する。
3. **`.rsrc` のサイズとエントロピーを測る**（§3）。現代の PNG ベース `.ico` を持つ GUI アプリは、コンソールアプリが持たない大きく圧縮不能な塊を抱えている。最も一般的な単一差分がこれ。
4. **バージョンメタデータを埋める** — CompanyName・LegalCopyright・OriginalFilename・説明的な FileDescription。欠落は実測で不利。Tauri では §4 の `tauri-winres` の罠に注意。
5. **1 ビルド 1 変数、かつ変更はビルド側で。**
6. **表を残す。** ハッシュ×変数×判定。**表こそが成果物**であり、表のない「通った 1 本」は再現できない幸運にすぎない。
7. **勝ちレシピはゼロから再ビルドして再スキャンする**（行 5）。信じるのはその後。
8. **誤検知申請は並行して出す。計画の本体にはしない。** ベンダーの応答は数週間単位で、買収されたベンダーの窓口は無期限に無人でありうる。
9. **なぜこの形なのかを記録する。** さもなくば、善意の次の担当者によって修正は取り消される。

---

## 9. 本件が示すこと・示さないこと

- 2 件の判定は、**埋め込まれた絵の性質と、自由記述のメタデータ欄の有無で決まっていた**。いずれもプログラムの挙動の性質ではなく、いずれも作者が——良性であれ悪性であれ——容易に調整できる。
- したがって、この種の ML 判定は **プログラムが何をするかについての証拠ではない**。利用者も、開発者も、集計スコアを引用する者も、そのように読むべきではない。
- **同じ結論は逆向きにも成立する。そして声を大にすべきはこちらである。** 本件の 0/70 は、プログラムをより安全にして得たものではない。**絵を再符号化して得たものである。** 2/70 から 0/70 へ 2 点動く間に、プログラムの実際の安全性に関わる性質は一つも変わっていない——コードセクションは同一である（§4.1）。**したがって、clean な集計スコアも、そのバイナリが安全であることの証拠ではない。** それは監査ではないし、監査であるかのように提示されてはならない。本プログラムの安全性について言えることは、別の手段で別の場所に記録されている——33 項目の出荷検査・SCA・ペネトレーションテストの playbook・SOT の設計制約であり、それは [security-assessment.md](./security-assessment.md) にあって、スキャン結果のページにはない。
- 露出は、ごく普通のソフトウェアの形状に対して系統的に発生する：**未署名の GUI アプリケーション＋現代的な高解像度アイコン**。この形状に異常な点は何もない。だからこそ同じ失敗が繰り返される。
- 本件は、ML 検知が無価値であること、これらのベンダーが悪意を持つこと、特定の閾値が意図的であることを **示さない**。示すのは、2 つの具体的なモデルが、測定可能な 1 事例において、ある時点で、何に反応していたかである。

---

## 10. 限界

- **時点情報。** ベンダーのモデルは継続的に変わる。本稿の判定は 2026-07-23/24 の観測であり、ハッシュによってのみ再検証可能。
- **1 製品・1 プラットフォーム。** Windows x86_64、Tauri アプリ 1 本。**手法**は一般化するが、具体的な閾値はしない可能性がある。
- **閾値は挟み込みであって二分探索していない。** Microsoft の `.rsrc` サイズ境界は 51,200 B（clean）と 297,472 B（flag）の間、Trapmine のエントロピー境界は 5.381（clean）と 7.928（flag）の間。
- **モデルは多特徴。** 特定できたのは **制御可能な**レバーであって特徴集合の全体ではない。変化させていない特徴は他にもある。
- **VirusTotal のエンジン構成はベンダーの出荷製品そのものではない** — 既定設定のローカル Defender は本稿のどのビルドも検知しなかった。
