# Security Policy

*(日本語は下にあります。)*

## Reporting a vulnerability

If you find a security issue in xoksa, please report it **privately** — do not open a public issue for an unfixed vulnerability.

- **Email:** info@xoksa.jp — subject line `SECURITY — xoksa`
- Please include: the affected version (`xoksa --version`), your platform (OS), a description of the issue, and reproduction steps or a proof of concept.
- We aim to acknowledge within a few business days and to coordinate a fix and a disclosure timeline with you.

Please act in good faith: test only against your own local instance, do not access data that isn't yours, and give us reasonable time to fix before any public disclosure. We appreciate coordinated disclosure and will credit reporters who wish to be named.

## Scope

- **In scope:** the `xoksa` binary and the local `xoksa serve` HTTP server + embedded Web UI in its **loopback default** (`127.0.0.1`).
- **Out of scope:** external services (Yahoo / Stooq / Brave / LLM providers). The exposed `--host 0.0.0.0` mode has, by design, no built-in auth/TLS/rate-limit and is the operator's responsibility — run it behind a reverse proxy that terminates TLS and enforces authentication.

## Supported versions

Security fixes target the **latest release**. See the [CHANGELOG](./CHANGELOG.md).

## Details

- Threat model & design-level controls → [docs/dev-prog/security-design.md](./docs/dev-prog/security-design.md)
- Point-in-time assessment & penetration-testing playbook → [docs/dev-prog/security-assessment.md](./docs/dev-prog/security-assessment.md)
- Per-release verification (inspected against the exact shipped binary's hash) → [docs/dev-prog/security-assessment.md](./docs/dev-prog/security-assessment.md) (Part C)

---

# セキュリティポリシー

## 脆弱性の報告

xoksa にセキュリティ上の問題を見つけた場合は、**非公開で**報告してください——未修正の脆弱性を公開 issue に書かないでください。

- **メール:** info@xoksa.jp（件名 `SECURITY — xoksa`）
- 記載してほしい内容: 対象バージョン（`xoksa --version`）、プラットフォーム（OS）、問題の説明、再現手順または PoC。
- 数営業日以内の受領連絡を目標とし、修正と開示のスケジュールを調整します。

誠実な行動をお願いします: 検証はご自身のローカル環境に対してのみ行い、自分のものでないデータにはアクセスせず、公開前に修正の時間を確保してください。コーディネートされた開示に感謝し、希望される報告者はクレジットします。

## 対象範囲

- **対象:** `xoksa` バイナリと、**ループバック既定**（`127.0.0.1`）のローカル `xoksa serve` HTTP サーバ＋埋め込み Web UI。
- **対象外:** 外部サービス（Yahoo / Stooq / Brave / LLM プロバイダ）。公開モード `--host 0.0.0.0` は設計上、認証/TLS/レート制限を持たず運用者の責任です——TLS 終端と認証を行う reverse proxy の背後で運用してください。

## 対応バージョン

セキュリティ修正は**最新リリース**を対象とします。[CHANGELOG](./CHANGELOG.md) を参照。

## 詳細

- 脅威モデルと設計レベルのコントロール → [docs/dev-prog/security-design.md](./docs/dev-prog/security-design.md)
- 時点の脆弱性診断とペネトレーションテスト playbook → [docs/dev-prog/security-assessment.md](./docs/dev-prog/security-assessment.md)
- リリースごとの検証（出荷する現物バイナリのハッシュに束縛） → [docs/dev-prog/security-assessment.md](./docs/dev-prog/security-assessment.md)（Part C）
