<!--
運用ポリシー / Operations policy: docs/dev-oper/github-operations-policy.md
下の項目は §4（必須）と §5-2（PR ライフサイクル）に対応します。
The sections below mirror §4 (MUST) and §5-2 (PR lifecycle) of that policy.
-->

## 目的 / Purpose

<!-- この PR が何を解決するか。1〜3 行。 -->

## 背景 / Background

<!-- なぜ今これが必要か。きっかけとなった事象・実測・レビュー指摘。 -->

## 変更点 / Changes

<!-- 区分は CHANGELOG と揃える: Added / Changed / Fixed / Security / Docs / Chore -->

| 区分 | 内容 |
| :--- | :--- |
|  |  |

## 影響 / Impact

<!-- 利用者から見た挙動の変化・互換性注意・移行の要否。無ければ「なし」と書く。 -->

## テスト手順・実測 / Tests and measurements

<!-- 再現できる形で書く。出荷検査を伴う PR は現物の SHA-256 を併記する。 -->

## 関連 / Links

<!--
Issue を紐づける。作業が残る Issue には Closes ではなく Refs を使う
（Closes はマージ時に自動クローズするため、残件があると取りこぼす）。
-->

Refs #

---

### 提出前チェック / Before requesting review

- [ ] 秘密情報を含まない（API キー・トークン・`xoksa.env`）
- [ ] 生成物を含まない（バイナリ・ビルド成果物・ログ・キャッシュ）
- [ ] `scripts/check.sh` が green
- [ ] 小さな差分に収まっている（目的が複数あるなら PR を分ける）
- [ ] ラベルを付けた（`type:*` ＋ `priority:*` ＋ `status:ready-for-review`）
