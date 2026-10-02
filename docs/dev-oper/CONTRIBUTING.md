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

### Overview

`.github/workflows/ci.yml` automatically runs quality checks on push and PR.

### Execution Timing

- Push to `main`, `xoksa-dev-**`, or `release/**`
- Open or update of a PR targeting `main`

### Check Contents

Job **Format / Lint / Test** (executed in order):

| Step | Command | Purpose |
|---|---|---|
| 1 | `cargo fmt -- --check` | Enforce uniform code formatting |
| 2 | `cargo clippy -- -D warnings` | Enforce zero warnings |
| 3 | `cargo test -q` | Verify all tests pass |

Job **cargo audit** (dependency vulnerabilities):

| Step | Command | Purpose |
|---|---|---|
| 1 | `cargo audit` | Native crate advisory scan |
| 2 | `cargo audit -f webui-leptos/Cargo.lock` | WASM frontend advisory scan |
| 3 | `cargo deny check` | Advisories **and** licence / source compliance |
| 4 | `cargo deny --manifest-path webui-leptos/Cargo.toml --config deny.toml check` | Same, for the WASM frontend |
| 5 | `cargo machete` | Unused dependencies |

`cargo deny` is the step most easily forgotten, and it fails on things the other
checks cannot see: a new transitive dependency can arrive under a licence the
allowlist has never been asked about. That is exactly what happened in #159 —
`reqwest` 0.13 pulled in `webpki-root-certs` under CDLA-Permissive-2.0 and left
`main` red, although `fmt`, `clippy` and `test` were all green locally.

### Notes

- Due to dependency on `native-tls`, `libssl-dev` is pre-installed in the CI environment (ubuntu-latest).
- Cargo build cache is saved/restored with `Cargo.lock` hash as the key (for speed).
- PRs with failing CI cannot be merged (linked with GitHub Branch Protection).

### Local Pre-check (equivalent to CI)

```bash
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test -q
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

**Rationale.** `kebab-case` is the de-facto standard for web-served and Markdown content (URLs are case-sensitive on Linux hosts; lowercase avoids "works on my Windows box, 404 in CI/prod" surprises), and the CSS/asset pipeline already used it. `snake_case` for `.rs` matches the Rust ecosystem so tooling and readers see no surprises. A single style per kind removes the "which case was that file again?" tax and the amateur look of a mixed tree.

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

### 概要

`.github/workflows/ci.yml` により、push および PR 時に品質チェックが自動実行されます。

### 実行タイミング

- `main` / `xoksa-dev-**` / `release/**` ブランチへの push
- `main` をベースとした PR のオープン・更新

### チェック内容

ジョブ **Format / Lint / Test**（順番に実行）:

| ステップ | コマンド | 目的 |
|---|---|---|
| 1 | `cargo fmt -- --check` | コードフォーマットの統一 |
| 2 | `cargo clippy -- -D warnings` | 警告ゼロの強制 |
| 3 | `cargo test -q` | 全テスト通過の確認 |

ジョブ **cargo audit**（依存の脆弱性スキャン）:

| ステップ | コマンド | 目的 |
|---|---|---|
| 1 | `cargo audit` | ネイティブ crate の脆弱性スキャン |
| 2 | `cargo audit -f webui-leptos/Cargo.lock` | WASM フロントエンドの脆弱性スキャン |
| 3 | `cargo deny check` | 脆弱性**および**ライセンス・ソースの適合 |
| 4 | `cargo deny --manifest-path webui-leptos/Cargo.toml --config deny.toml check` | 同じことを WASM フロントエンドに対して |
| 5 | `cargo machete` | 未使用の依存 |

`cargo deny` は最も忘れやすく、かつ他のチェックでは見えないものに落ちる。新しい推移的
依存が、許可リストに一度も問われたことのないライセンスで入ってくることがあるためである。
#159 がまさにそれで、`reqwest` 0.13 が `webpki-root-certs` を CDLA-Permissive-2.0 で
引き込み、手元では `fmt`・`clippy`・`test` がすべて green だったにもかかわらず `main` の
CI を赤にした。

### 注意点

- `native-tls` ライブラリの依存により、CI 実行環境（ubuntu-latest）では `libssl-dev` を事前インストールしています。
- Cargo のビルドキャッシュは `Cargo.lock` のハッシュをキーに保存・復元します（高速化）。
- CI が失敗している PR はマージ禁止（GitHub の Branch Protection と連動）。

### ローカルでの事前確認（CI と同等）

```bash
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test -q
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

**理由。** `kebab-case` は Web 配信・Markdown コンテンツの事実上の標準であり（URL は Linux ホストで大文字小文字を区別する。小文字統一は「自分の Windows では動くが CI/本番で 404」を避ける）、CSS・アセットのパイプラインでも既に使っている。`.rs` の `snake_case` は Rust エコシステムに一致し、ツールにも読み手にも意外性がない。種別ごとに1スタイルへ統一することで「あのファイルはどの表記だったか」という無駄と、混在したツリーの素人臭さを取り除く。

**新規ファイルへの適用。** 上の表に合わせる。コード以外の資産で迷ったら `kebab-case`。一度きりの都合で新しい表記を持ち込まない。必要なら合意のうえでこの表を拡張する。

---
