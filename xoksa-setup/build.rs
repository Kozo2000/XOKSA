// Windows: tauri-build embeds icons/icon.ico into the PE resource section (.rsrc).
// That section's SIZE and ENTROPY are what anti-malware ML engines score, so the
// icon must stay UNCOMPRESSED (BMP frames) and SMALL (max 64x64). The icons here
// are copied from the desktop app's AV-verified set; do not swap in PNG/256x256
// frames. Measured method: docs/dev-prog/av-false-positive-case-study.md
fn main() {
    tauri_build::build();
}
