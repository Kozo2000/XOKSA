## Operations Memo (Branch / PR / Merge)

[日本語ドキュメントはこちら。](#ja)

> **Language sync:** English and Japanese descriptions are both authoritative. If a requirement, restriction, exception, command behavior, or operational rule appears in only one language, treat it as applying to both; the missing side is a documentation defect to be updated.

- Default branch: `main`
- Prohibited: direct push / force-push to `main` (always via PR)
- Branch naming:
  - Feature addition: `feat/<short-name>`
  - Bug fix: `fix/<short-name>`
  - Documentation: `docs/<short-name>`
  - Housekeeping: `chore/<short-name>`
  - Version release: `release/v<X.Y.Z>` (dotted version, e.g. `release/v2.9.0`)

### Flow
1) Create working branch: `git switch -c feat/<name>`
2) Small commits (`type: summary`, e.g., `feat: add 5-step gauge`)
3) Push: `git push -u origin feat/<name>`
4) **Create PR** (title = summary, body = purpose/impact/verification steps)
5) **Checks (required)**
   - `cargo fmt -- --check` (formatting OK)
   - `cargo clippy -D warnings` (zero warnings)
   - `cargo test -q` (all tests pass)
   - `cargo deny check` (advisories **and** licence / source compliance — the step
     that is easiest to skip and the one that caught nothing locally in #159)
   - `cargo build --release` (build succeeds)
   - **Separate workspaces:** `webui-leptos/` and `xoksa-desktop/` are their own Cargo workspaces — the root `cargo …` does not cover them. If your change touches one, run its checks there too (e.g. `cd xoksa-desktop && cargo fmt -- --check && cargo clippy -- -D warnings && cargo build`); a signed, distributable desktop bundle is `cargo tauri build`.
   - Screen output format/color/width intact (screenshot if needed)
6) **Merge method**: green button ▾ → **Squash and merge** by default (Non-FF requires presenting the reason and obtaining agreement)
   Squash message: `type: summary (#PR-number)`
7) After merge
   - Remote source branch is auto-deleted (configured)
   - Local: `git branch -d <branch> && git fetch -p`
   - Update: `git switch main && git pull`

### Review Criteria (Self-check for solo operation)
- Matches specification and output examples (Japanese labels / gauge width / colors)
- Changes are **single-responsibility, minimal diff** (no surrounding optimization, no renaming)
- No regression on zero-division guards, etc. (Buyer/Seller=50%, Holder=0%)
- New files follow the naming convention below (kebab-case for docs/assets, snake_case for `.rs`)

---

## Automated Tests

### Overview

This project uses Rust's standard `#[test]` / `#[cfg(test)]` framework for automated testing.
No external testing frameworks are used.

### Commands

```bash
cargo test          # Run all tests (verbose output)
cargo test -q       # Run all tests (concise output)
cargo test <name>   # Run only tests matching the name (e.g., cargo test ema_score)
```

Tests are not included in the release binary and do not affect end-user behavior.

### Test Configuration

Tests live beside the code as `#[cfg(test)]` modules across the engine (indicators,
composite, bootstrap, config, backtest, logging, server, db, …) plus the
cross-cutting suite in `tests/integration_test.rs` and the no-drift gate in
`tests/no_drift.rs`. The suite grows with the code,
so `cargo test` is the source of truth for the current count.

**Current totals: 425 library unit tests (424 pass, 1 ignored: needs network) + 10
integration tests + 5 no-drift tests** (run `cargo test` to confirm).

### Test Design Policy (3 Layers)

| Layer | Purpose | Target |
|---|---|---|
| Layer 1 | Score judgment logic boundary values | Directly test `pub(crate)` pure functions |
| Layer 2 | Error paths for invalid input / insufficient data | Verify `Err` with short data and conflicting parameters |
| Layer 3 | Aggregation of multiple indicators and final output | Build `TechnicalDataGuard` and verify integrated output. For the fundamental boundary, build through `FundamentalData::build` / `set_bps` / `recompute_derived` and verify that a non-finite figure is refused, that a legitimate zero and a meaningful negative survive, and that the derived ratios stay consistent with the inputs and the price they came from |

### Rules for Adding Tests

- When adding a new function containing score judgment logic, separate the judgment logic as a `pub(crate)` pure function and always add Layer 1 tests.
- When adding new input sanitization, add rejection tests for injection characters (`;` `|` `` ` ``).
- When new cross-module use cases are added, add integration tests in `tests/integration_test.rs`.
- Confirm that `cargo test -q` passes all tests before PR merge (see Check step 5).

---

## CI (GitHub Actions)

The developer runs CI once to check and build the program for macOS and Windows when the program’s source code or dependency libraries change. If neither changes, the developer does not rerun CI or rebuild the program. Windows signing and packaging continue in the same run after owner approval. The order and protection settings are in [the operations policy](github-operations-policy.md) §5-4.

`scripts/check.sh` runs in the OS jobs before artifact upload. It requires `cargo-audit`, `cargo-deny` and `cargo-machete`; a missing tool or failed check fails the job.

| Check | Scope |
| :--- | :--- |
| fmt, clippy | Engine, WASM frontend, desktop, settings app, paths |
| test | Engine, desktop, settings app, paths |
| audit, deny | All five workspaces; deny uses the native host target or `wasm32-unknown-unknown` as appropriate |
| machete | Repository manifests |
| Build and payload gates | Both OS artifacts, locked dependencies, sidecar versions and identifying build paths |

Local pre-push checks use the same script and require the generated frontend and sidecars. They do not replace CI. A failed or incomplete CI run blocks publication.

```bash
bash scripts/check.sh
```

## File & Directory Naming Convention

One rule per file kind. No mixing of `camelCase`, `PascalCase`, `snake_case`, and `kebab-case` within the same category — the repository picks exactly one style per kind and holds to it.

| Kind | Style | Example |
|------|-------|---------|
| Rust source (`.rs`) | `snake_case` | `src/chat/exec.rs`, `src/market.rs` |
| Docs (`.md`) | `kebab-case`, all lowercase | `docs/dev-prog/security-design.md` |
| Directories | `kebab-case`, all lowercase | `docs/dev-prog/`, `docs/manual/` |
| Images / assets | `kebab-case`, all lowercase | `docs/images/xoksa-en.png` |
| Stylesheets (`.css`) | `kebab-case` | `webui-leptos/xoksa-chart.css` |
| Shell scripts (`.sh`) | `kebab-case` | `scripts/inspect.sh` |

Version tags inside a name keep their dotted form (e.g. a hypothetical `report-v2.3.0.md`).

**Exceptions — leave these as-is.** These names are dictated by external tooling or ecosystem convention; changing them breaks the tool or the platform's special handling:

- **GitHub-special files** rendered by convention: `README.md`, `LICENSE`, `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md` (uppercase is the recognized form — GitHub surfaces `SECURITY.md` in the Security tab).
- **Tooling-fixed files**: `Cargo.toml`, `Cargo.lock` (cargo requires the exact name), `.gitignore`, `.gitattributes`, workflow YAML under `.github/`.
- **Rust source stays `snake_case`** — that is the language convention, not a deviation from this rule.

**Applying to new files.** Match the table above. When in doubt for a non-code asset, choose `kebab-case`. Do not introduce a new casing style for a one-off — extend this table by agreement instead.

---

<a id="ja"></a>

## 運用メモ（ブランチ/PR/マージ）

> **言語同期:** 英語と日本語の記載はいずれも有効です。要件・制約・例外・コマンド挙動・運用ルールが片方の言語にだけ存在する場合でも、両方に適用されます。欠落している側は文書不備として更新対象です。

- 既定ブランチ: `main`
- 禁止: `main` への直接 push / force-push（常に PR 経由）
- ブランチ命名:
  - 機能追加: `feat/<短い名前>`
  - バグ修正: `fix/<短い名前>`
  - 文書: `docs/<短い名前>`
  - 雑務/整備: `chore/<短い名前>`
  - バージョンリリース: `release/v<X.Y.Z>`（ドット版、例: `release/v2.9.0`）

### フロー
1) 作業ブランチ作成 `git switch -c feat/<name>`
2) 小さくコミット（`type: 要約` 例: `feat: 5段階ゲージ導入`）
3) プッシュ `git push -u origin feat/<name>`
4) **PR を作成**（タイトル=要約、本文=目的/影響/確認手順）
5) **チェック（必須）**
   - `cargo fmt -- --check`（整形OK）
   - `cargo clippy -D warnings`（警告ゼロ）
   - `cargo test -q`（全テスト通過）
   - `cargo deny check`（脆弱性**および**ライセンス・ソースの適合。最も飛ばしやすく、
     #159 では手元の他のチェックが何も捕まえられなかった段）
   - `cargo build --release`（ビルド成功）
   - **別 workspace:** `webui-leptos/` と `xoksa-desktop/` は独立した Cargo workspace で、ルートの `cargo …` では対象外。いずれかに触れた変更は、その中でもチェックを実行（例: `cd xoksa-desktop && cargo fmt -- --check && cargo clippy -- -D warnings && cargo build`）。署名付き配布バンドルは `cargo tauri build`。
   - 画面出力の体裁/色/幅に破綻なし（必要ならスクショ）
6) **マージ方法**: 緑ボタンの ▾ → **Squash and merge** を原則（Non-FF はその理由を提示し合意を得ること）
   スカッシュメッセージ: `type: 要約 (#PR番号)`
7) マージ後
   - リモート元ブランチは自動削除（設定済）
   - ローカル: `git branch -d <branch> && git fetch -p`
   - 最新化: `git switch main && git pull`

### レビュー基準（ソロ運用の自己チェック）
- 仕様と出力例に一致（日本語ラベル/ゲージ幅/色）
- 変更は**単一責務・最小差分**（周辺最適化・リネーム禁止）
- 0除算ガード等の退行なし（Buyer/Seller=50%、Holder=0%）
- 新規ファイルは後述の命名規約に準拠（docs/資産は kebab-case、`.rs` は snake_case）

---

## 自動テスト

### 概要

本プロジェクトは Rust 標準の `#[test]` / `#[cfg(test)]` を用いた自動テストを整備しています。
外部テストフレームワークは使用しません。

### 実行コマンド

```bash
cargo test          # 全テストを実行（詳細出力）
cargo test -q       # 全テストを実行（結果のみ簡潔に表示）
cargo test <名前>   # 名前に部分一致するテストのみ実行（例: cargo test ema_score）
```

テストはリリースバイナリに含まれません。エンドユーザーの動作には影響しません。

### テスト構成

テストはコードと同じ場所に `#[cfg(test)]` モジュールとして配置され、エンジン全体
（indicators・composite・bootstrap・config・backtest・logging・server・db …）＋
横断的な `tests/integration_test.rs` と、ドリフト検知の `tests/no_drift.rs` に分散
しています。コードの増加に伴い件数も増える
ため、現在の件数は `cargo test` が正（source of truth）です。

**現在の合計: ライブラリ ユニット 425件（424件成功・1件は要ネットワークのため ignore）＋ 統合 10件 ＋ no-drift 5件**（`cargo test` で確認可能）。

### テスト設計方針（3層）

| Layer | 目的 | 対象 |
|---|---|---|
| Layer 1 | スコア判定ロジックの境界値 | `pub(crate)` 純粋関数を直接テスト |
| Layer 2 | 不正入力・データ不足のエラーパス | 短データ・矛盾パラメータで `Err` を確認 |
| Layer 3 | 複数指標の集計・最終出力 | `TechnicalDataGuard` を組み立てて統合確認。ファンダメンタル側の境界は `FundamentalData::build`／`set_bps`／`recompute_derived` を通して組み立て、非有限値が弾かれること・正常なゼロと意味のある負値が残ること・派生比率が入力および元になった価格と整合することを確認する |

### テスト追加のルール

- スコア判定を含む関数を新規追加する場合は、判定ロジックを `pub(crate)` 純粋関数として分離し、Layer 1 テストを必ず追加する。
- 入力サニタイズを新規追加する場合は、インジェクション文字（`;` `|` `` ` ``）の拒否テストを追加する。
- モジュールをまたぐ新しいユースケースが増えた場合は `tests/integration_test.rs` に統合テストを追加する。
- `cargo test -q` が全件通過することを PR マージ前に確認すること（チェック手順 5 参照）。

---

## CI（GitHub Actions）

開発者は、プログラムのソースコードまたは依存ライブラリを変更したとき、CIを1回実行してmacOSとWindowsの検査とビルドを行う。ソースコードと依存ライブラリを変更していなければ、CIやビルドをやり直さない。Windowsの署名待ちと梱包も、オーナーの承認後に同じ実行で続ける。工程と保護設定は[運用ポリシー](github-operations-policy.md) §5-4 に記載する。

OS別ジョブ内で、成果物を出す前に `scripts/check.sh` を実行する。`cargo-audit`・`cargo-deny`・`cargo-machete` は必須で、不足や検査失敗があればジョブを失敗にする。

| 検査 | 対象 |
| :--- | :--- |
| fmt・clippy | エンジン、WASMフロントエンド、デスクトップ、設定アプリ、paths |
| test | エンジン、デスクトップ、設定アプリ、paths |
| audit・deny | 全5workspace。denyは各ネイティブhostまたは `wasm32-unknown-unknown` を対象にする |
| machete | リポジトリ内のmanifest |
| ビルドと現物のゲート | 両OSの現物、固定依存、sidecarの版数、個人のビルドパス |

push前のローカル検査でも同じスクリプトを使い、生成済みフロントエンドとsidecarを必要とする。ローカル検査でCIを代替しない。CIが失敗または未完了の場合は公開しない。

```bash
bash scripts/check.sh
```

## ファイル・ディレクトリ命名規約

ファイル種別ごとに規則は1つ。同じカテゴリ内で `camelCase`・`PascalCase`・`snake_case`・`kebab-case` を混在させない。種別ごとにちょうど1つのスタイルを選び、それを守る。

| 種別 | スタイル | 例 |
|------|-------|---------|
| Rust ソース（`.rs`） | `snake_case` | `src/chat/exec.rs`・`src/market.rs` |
| ドキュメント（`.md`） | `kebab-case`（すべて小文字） | `docs/dev-prog/security-design.md` |
| ディレクトリ | `kebab-case`（すべて小文字） | `docs/dev-prog/`・`docs/manual/` |
| 画像・アセット | `kebab-case`（すべて小文字） | `docs/images/xoksa-en.png` |
| スタイルシート（`.css`） | `kebab-case` | `webui-leptos/xoksa-chart.css` |
| シェルスクリプト（`.sh`） | `kebab-case` | `scripts/inspect.sh` |

名前に含めるバージョンタグはドット表記のまま（例：仮に `report-v2.3.0.md`）。

**例外 — そのままにするもの。** 外部ツールやエコシステムの慣習で名前が決まっているもの。変更するとツールやプラットフォームの特別扱いが壊れる：

- **GitHub が特別扱いするファイル**：`README.md`・`LICENSE`・`CHANGELOG.md`・`CONTRIBUTING.md`・`SECURITY.md`（大文字が認識される形式。GitHub は `SECURITY.md` を Security タブに表示する）。
- **ツール固定のファイル**：`Cargo.toml`・`Cargo.lock`（cargo が正確な名前を要求）・`.gitignore`・`.gitattributes`・`.github/` 配下のワークフロー YAML。
- **Rust ソースは `snake_case` のまま** — 本規約からの逸脱ではなく、言語側の慣習。

**新規ファイルへの適用。** 上の表に合わせる。コード以外の資産で迷ったら `kebab-case`。一度きりの都合で新しい表記を持ち込まない。必要なら合意のうえでこの表を拡張する。

---
