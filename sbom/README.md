# SBOM (Software Bill of Materials)

Machine-readable dependency inventories in **CycloneDX 1.5** (JSON), for supply-chain / SCA tooling.

| File | Covers | Components |
|------|--------|-----------|
| `xoksa-native-windows-x86_64.cdx.json` | the shipped Windows engine (`xoksa.exe`), `cargo build --release` (default `embedded-ui`) | 225 cargo crates |
| `xoksa-native-macos-arm64.cdx.json` | the shipped macOS engine (`xoksa`, inside `XOKSA.app`), same build | 221 cargo crates |
| `xoksa-desktop-windows-x86_64.cdx.json` | the shipped Windows desktop shell (`xoksa-desktop.exe`), the Tauri/WebView2 set | 196 cargo crates |
| `xoksa-setup-windows-x86_64.cdx.json` | the shipped Windows settings app (`xoksa-setup.exe`), Tauri/WebView2 plus the config and keychain paths | 212 cargo crates |
| `xoksa-webui-leptos.cdx.json` | the Rust/WASM frontend (`xoksa-webui`) | 188 cargo crates |

**One SBOM per OS is required, not redundant.** The two native inventories come from the same source and the same `Cargo.lock`, but a `Cargo.lock` is the union over all targets while an SBOM lists what is **in that binary**. The platform-specific dependencies swap out: Windows carries `schannel` / `winapi` / `windows-sys` / `windows-native-keyring-store`, macOS carries `security-framework` / `core-foundation` / `apple-native-keyring-store`. Publishing only one would have downstream SCA scanning crates that are not in the artifact and missing the ones that are. The WASM frontend needs only one file — its target (`wasm32-unknown-unknown`) does not depend on the build host.

## Regenerating

These are **version-specific** — regenerate on any dependency change and per release, on each shipping OS:

```bash
# 1) throwaway build that embeds the dependency list (NEVER ship this binary).
#    Use a SEPARATE target dir so target/release/xoksa — the binary already
#    inside a built MSI — is not replaced (MSVC builds are not reproducible,
#    so a rebuild would no longer match the shipped payload).
cargo auditable build --release --bin xoksa --target-dir target-sbom

# 2) read the embedded list from a NEUTRAL directory and emit CycloneDX.
#    syft records the path of the file it reads, so copy the binary out of the
#    workspace first and invoke it by a relative name — never edit the output.
#    Pin the spec version: syft defaults to 1.7, these inventories are 1.5.
#    SYFT_FILE_METADATA_SELECTION=none suppresses the file-metadata cataloguer,
#    which would otherwise add a `file` component naming the scanned binary by
#    its RESOLVED ABSOLUTE path — on macOS that is where /tmp resolves to, e.g.
#    `/private/tmp/…/xoksa`. An inventory lists components; it is not a record of
#    where one machine happened to put the binary.
cp target-sbom/release/xoksa /tmp/sbomwork/xoksa && cd /tmp/sbomwork
SYFT_FILE_METADATA_SELECTION=none \
  syft file:xoksa -o cyclonedx-json@1.5=xoksa-native-<os>-<arch>.cdx.json

# 3) binary-bound advisory check on the same throwaway (the OS's real dependency set)
cargo audit bin xoksa

# 4) drop the throwaway tree; the shipped binary in target/release is untouched
rm -rf target-sbom
```

**On Windows, step 2's copy cannot be made with `cp`.** Git Bash resolves `xoksa` to
the existing `xoksa.exe` — for `cp` and for a shell redirect alike — so both write
`xoksa.exe` instead of the extensionless name, and report success. It matters because
syft's `pe-binary-package-cataloger` fires on the `.exe` extension and adds a further
component of type `application` (carrying a CPE and a relative location), which would
leave that inventory the only one of the five that is not `library`-only. Make the copy
with PowerShell instead — the same applies to `xoksa-desktop` and `xoksa-setup` below:

```powershell
Copy-Item -LiteralPath target-sbom\release\xoksa.exe -Destination <neutral-dir>\xoksa -Force
```

**WASM frontend** — syft cannot read `cargo auditable` data out of a `.wasm`, so this
one is catalogued from the lockfile instead (`rust-cargo-lock-cataloger`). The crate
is its own workspace, so its `Cargo.lock` resolves for `wasm32-unknown-unknown` only:

```bash
mkdir -p /tmp/sbomwork/xoksa-webui
cp webui-leptos/Cargo.lock webui-leptos/Cargo.toml /tmp/sbomwork/xoksa-webui/
cd /tmp/sbomwork
SYFT_FILE_METADATA_SELECTION=none \
  syft xoksa-webui -o cyclonedx-json@1.5=xoksa-webui-leptos.cdx.json
```

**Desktop shell and settings app** — separate workspaces with their own
`Cargo.lock`, so a root build never covers them. Same throwaway-build method as
the engine, once per binary (§6 requires an inventory for each once it ships):

```bash
# run once in xoksa-desktop, then once in xoksa-setup (substitute the name)
cd xoksa-desktop
cargo auditable build --release --bin xoksa-desktop --target-dir target-sbom
cp target-sbom/release/xoksa-desktop /tmp/sbomwork/xoksa-desktop && cd /tmp/sbomwork
syft file:xoksa-desktop -o cyclonedx-json@1.5=xoksa-desktop-windows-x86_64.cdx.json
cargo audit bin target-sbom/release/xoksa-desktop   # binary-bound advisories
rm -rf target-sbom
```

`target-sbom/` is gitignored in every workspace, so the throwaway tree cannot
reach the index even before step 4 removes it.

`cargo-cyclonedx` is **not used**: it writes `bom-ref path+file:///<absolute workspace path>` with no way to suppress it, and post-editing tool output is rejected on principle. Method rationale: [security-assessment.md §C.3](../docs/dev-prog/security-assessment.md).

## Caveats

- **Host target.** Each inventory resolves for the target it was built on; that is why the file name carries the OS and arch. Regenerate on the machine that produces the shipped artifact.
- **The table counts parts, not the product.** Every inventory names the built artifact in `metadata.component` (type `file`, taken from the path syft read); the crate counts above are the **`library` components** in `components[]`, and every inventory is now `library` only — macOS 221 of 221, Windows 225 of 225, WASM 188 of 188, the desktop shell 196 of 196 and the settings app 212 of 212. Step 2 still reads the throwaway from a neutral directory: the recorded path must not be a developer's home or the working tree. *(Corrected 2026-09-25: this note previously described the Windows entry as type `application` and counted 212 of 213; neither matched the published inventory.)* *(2026-09-27: the macOS inventory used to carry a 222nd component of type `file` naming the scanned binary by absolute path — `/private/tmp/sbom298/xoksa`, from the file-metadata cataloguer the recipe now disables. What is inside the binary did not change; the inventories stopped disagreeing about shape.)*
- **Point-in-time.** An SBOM reflects the lockfile at generation time. The authoritative vulnerability posture is the per-release SCA (`cargo audit` + `osv-scanner`, plus `cargo audit bin` against the artifact's own embedded list) recorded in [security-assessment.md](../docs/dev-prog/security-assessment.md); the SBOM is the component inventory those scans run against.

---

# SBOM（ソフトウェア部品表）

サプライチェーン / SCA ツール向けの機械可読な依存インベントリ（**CycloneDX 1.5** JSON）。

| ファイル | 対象 | コンポーネント |
|------|------|-----------|
| `xoksa-native-windows-x86_64.cdx.json` | 出荷 Windows エンジン（`xoksa.exe`）、`cargo build --release`（既定 `embedded-ui`） | 225 crate |
| `xoksa-native-macos-arm64.cdx.json` | 出荷 macOS エンジン（`XOKSA.app` 内の `xoksa`）、同じビルド | 221 crate |
| `xoksa-desktop-windows-x86_64.cdx.json` | 出荷 Windows デスクトップシェル（`xoksa-desktop.exe`）、Tauri/WebView2 一式 | 196 crate |
| `xoksa-setup-windows-x86_64.cdx.json` | 出荷 Windows 設定アプリ（`xoksa-setup.exe`）、Tauri/WebView2 に設定・キーチェーン経路を加えたもの | 212 crate |
| `xoksa-webui-leptos.cdx.json` | Rust/WASM フロントエンド（`xoksa-webui`） | 188 crate |

**OS ごとの SBOM は冗長ではなく必須である。** 2 つのネイティブ・インベントリは同一ソース・同一 `Cargo.lock` から生成されるが、`Cargo.lock` が全ターゲットの和集合であるのに対し、SBOM は**そのバイナリに入っているもの**を列挙する。プラットフォーム依存の依存関係は丸ごと入れ替わる：Windows は `schannel` / `winapi` / `windows-sys` / `windows-native-keyring-store`、macOS は `security-framework` / `core-foundation` / `apple-native-keyring-store`。片方だけを公開すると、下流の SCA は現物に入っていない crate を検査し、入っている crate を見落とす。WASM フロントエンドは 1 本でよい（ターゲット `wasm32-unknown-unknown` はビルドホストに依存しないため）。

## 再生成

**バージョン依存**のため、依存変更時・リリース毎に、出荷する OS それぞれで再生成すること（コマンドは上記 English 節と同一）。要点は 5 つ：

1. 依存リストを埋め込む使い捨てビルドを、**別の `--target-dir`** で行う（この現物は絶対に出荷しない。`target/release` を上書きすると、既に MSI に入っている現物とハッシュが食い違う。MSVC ビルドは再現しないため）。
2. 中立ディレクトリへコピーしてから `syft file:` で読む（syft は読んだファイルのパスを記録するため。出力の後編集は禁止）。
3. 仕様バージョンを **`@1.5` で固定**し、**`SYFT_FILE_METADATA_SELECTION=none`** を付ける。付けないと、走査したバイナリを**解決済みの絶対パス**で名指す `file` コンポーネントが 1 つ増える（macOS では `/tmp` の解決先、例 `/private/tmp/…/xoksa`）。インベントリは部品の一覧であって、あるマシンがバイナリをどこに置いたかの記録ではない。
4. 同じ使い捨てに `cargo audit bin` を掛けて、現物ベースの助言チェックを取る。
5. 使い捨ての target-dir を削除する。

**Windows では手順 2 のコピーを `cp` で作れない。**
Git Bash（MSYS）は `xoksa` を既存の `xoksa.exe` に解決する——`cp` でも
シェルのリダイレクトでも同じ——ので、どちらも拡張子なしの
名前ではなく `xoksa.exe` に書き、成功したと報告する。これが問題に
なるのは、syft の `pe-binary-package-cataloger` が `.exe` 拡張子で発動し、
type `application` のコンポーネント（CPE と相対パスを伴う）を 1 つ
余計に足すためで、そうなるとそのインベントリだけが 5 本の中で
`library` のみでなくなる。コピーは PowerShell で行うこと——下の
`xoksa-desktop`・`xoksa-setup` も同じである：

```powershell
Copy-Item -LiteralPath target-sbom\release\xoksa.exe -Destination <中立ディレクトリ>\xoksa -Force
```

**WASM は例外。** `.wasm` からは syft が `cargo auditable` のデータを読めないため、`Cargo.lock` を読ませる方式（`rust-cargo-lock-cataloger`）で生成する。webui-leptos は独立 workspace なので、その `Cargo.lock` は `wasm32-unknown-unknown` 向けにのみ解決される。
**デスクトップシェルと設定アプリも、それぞれ独立 workspace である。** 自前の `Cargo.lock` を持ち、ルートのビルドでは覆えないので、エンジンと同じ使い捨て方式をバイナリごとに 1 回ずつ行う（§6 は、出荷した時点で各々のインベントリを要求する）。`target-sbom/` は全 workspace で gitignore 済みなので、手順 5 で削除する前でも索引には入らない。


`cargo-cyclonedx` は**使用しない**（`bom-ref` に `path+file:///<絶対パス>` を書き込み、抑制手段がない。ツール出力の後編集も原則禁止）。方式の根拠は [security-assessment.md §C.3](../docs/dev-prog/security-assessment.md)。

## 注意点

- **host ターゲット。** 各インベントリはビルドしたターゲットで解決される。ファイル名に OS とアーキテクチャを含めているのはそのため。出荷現物を作るマシンで再生成すること。
- **表が数えているのは部品であって製品ではない。** どのインベントリも、ビルドした成果物自身は `metadata.component` に載る（type `file`。syft が読んだファイルのパスを名前にする）。上表の crate 数は `components[]` の **`library` コンポーネント**であり、現在はどのインベントリも `library` のみで構成される——macOS は 221 中 221、Windows は 225 中 225、WASM は 188 中 188、デスクトップシェルは 196 中 196、設定アプリは 212 中 212。手順 2 で中立ディレクトリから読ませる理由は変わらない。記録されるパスに開発者のホームや作業ツリーが出てはならない。*（2026-09-25 訂正：本注記は Windows の項目を type `application`、213 中 212 と記していたが、どちらも公開済みのインベントリと一致していなかった。）* *（2026-09-27：macOS のインベントリには、走査したバイナリを絶対パス `/private/tmp/sbom298/xoksa` で名指す 222 番目の `file` コンポーネントが入っていた。file-metadata カタロガによるもので、現在の手順では無効化している。バイナリの中身の数は変わっておらず、インベントリ間で形が食い違わなくなっただけである。）*
- **時点情報。** SBOM は生成時点の lockfile を反映する。脆弱性の正となる姿勢は毎リリースの SCA（`cargo audit`＋`osv-scanner`、および現物の埋め込みリストに対する `cargo audit bin`。[security-assessment.md](../docs/dev-prog/security-assessment.md) に記録）であり、SBOM はそのスキャン対象となるコンポーネントの目録である。
