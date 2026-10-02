// Windows: tauri-build embeds icons/icon.ico into the PE resource section (.rsrc).
// That section's SIZE and ENTROPY are what two anti-malware ML engines score, so the
// icon must stay UNCOMPRESSED (BMP frames) and SMALL (max 64x64 — one 256x256 BMP
// frame is 262 KB by itself). Restoring PNG frames or a 256x256 frame re-flags this
// binary on VirusTotal; so does dropping CREATE_NO_WINDOW in src/main.rs or the PE
// version metadata. Measured per build: docs/dev-prog/av-false-positive-case-study.md
fn main() {
    tauri_build::build();
}
