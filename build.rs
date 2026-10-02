//! Build script: embed Windows PE metadata (version info + application manifest).
//!
//! Runs only when building for Windows — both the `winresource` build-dependency
//! and the code below are gated on `cfg(windows)`, so non-Windows builds
//! (including the Linux CI) compile an empty `main()` and pull in no extra deps.
//!
//! Embedding a version resource and an `asInvoker` manifest gives the binary the
//! standard publisher/product/version fields and an explicit, non-elevating
//! execution level. An unsigned CLI with no such metadata reads as more
//! suspicious to antivirus/heuristic scanners; this narrows that surface. It is
//! NOT a substitute for code signing.

fn main() {
    // Re-run this build script ONLY when build.rs itself changes. Without this,
    // a build script that emits no `rerun-if-changed` makes cargo watch the
    // ENTIRE package directory — so regenerating sbom/*.cdx.json or editing docs
    // would needlessly re-run the script and rebuild the crate. build.rs reads
    // nothing else from the tree (only CARGO_PKG_VERSION, which is already part
    // of the build-script fingerprint), so watching build.rs alone is exact.
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    embed_windows_metadata();
}

#[cfg(windows)]
fn embed_windows_metadata() {
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string());

    // `{{` / `}}` are escaped literal braces (the supportedOS GUIDs); `{version}`
    // is interpolated so the manifest identity tracks Cargo.toml automatically.
    let manifest = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="xoksa" version="{version}.0" processorArchitecture="*"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}}"/>
      <supportedOS Id="{{1f676c76-80e1-4239-95bb-83d0f6d0da78}}"/>
      <supportedOS Id="{{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}}"/>
      <supportedOS Id="{{35138b9a-5d96-4fbd-8e2d-a2440225f93a}}"/>
    </application>
  </compatibility>
</assembly>
"#
    );

    let mut res = winresource::WindowsResource::new();
    res.set("ProductName", "xoksa");
    res.set(
        "FileDescription",
        "Stock analysis and news title triage tool using LLMs",
    );
    res.set("CompanyName", "kozo2000");
    res.set(
        "LegalCopyright",
        "Copyright (c) 2026 Kozo2000 - MIT License",
    );
    res.set("OriginalFilename", "xoksa.exe");
    res.set_manifest(&manifest);

    if let Err(e) = res.compile() {
        // Don't fail the build if the resource compiler (rc.exe / windres) is
        // unavailable — warn visibly and ship the binary without the resource.
        println!("cargo:warning=failed to embed Windows resource metadata: {e}");
    }
}
