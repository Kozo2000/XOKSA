# xoksa GitHub Operations Policy

[日本語ドキュメントはこちら。](#ja)

> **Language sync:** English and Japanese descriptions are both authoritative. If a requirement, restriction, exception, command behavior, or operational rule appears in only one language, treat it as applying to both; the missing side is a documentation defect to be updated.

## 1. Basic rules

- Prevent misoperations, history destruction and secret exposure.
- Keep the actor, content and time of operations traceable.
- Define reproducible work procedures.
- Do not change the process through exceptions or verbal discretion outside this policy.
- Limit changes to the required scope.

---

## 2. Terminology / Abbreviations (Meanings Fixed in This Document)

### 2.1 Japanese Terms (Alphabetical | Definitions Only)

- Signal (合図): A description that clearly communicates a progress state change to the other party. Here, label updates on PRs and mentions in PR comments.
- Bypass (迂回): Reaching a blocked operation by another command, API, or tool.
- Scope of impact (影響範囲): The collection of targets, locations, and parties affected by the change (including direct and secondary effects).
- Acceptance criteria (受入基準): Text defining the conditions for considering an Issue/PR complete (including expected behavior, verification points, and exclusions).
- Compatibility note (互換性注意): A note explicitly indicating points that require adjustment or migration for existing usage or data.
- Deadline (期日): The agreed-upon deadline for that work.
- Visualization (可視化): Placing progress, order, and state in a form that anyone can check from the same location (e.g., labels, queue lists).
- Conflict (競合): A state of diff conflict that cannot be automatically merged.
- Code owner (コードオーナー): Definition of responsible reviewers per path.
- Commit ID (コミットID): The identifier (SHA) for a change.
- Commit creation date (コミット作成日時): The time the commit was created (for reference).
- Commit incorporation date (コミット取り込み日時): The time the commit was incorporated into the history (basis for judgment).
- Draft pull request (下書きプルリクエスト): A PR in progress (used for visibility of progress).
- Small PR (小さなPR): The concept of submitting in small diff units (specific thresholds according to project definition).
- Approval (承認): The state where a reviewer has judged incorporation as acceptable.
- Stale approval (承認失効): The state where an existing approval becomes invalid due to addition of new commits or base incorporation.
- Request for changes (修正要求): The state where a reviewer is requesting changes.
- Important change (重要変更): A collective term for changes that can have significant impact on users, operations, costs, security, or compatibility.
- Artifact (生成物): A collective term for outputs that can be regenerated from source (builds, binaries, logs, caches, etc.).
- Tag / Release (タグ／リリース): A marker/distribution unit pointing to the state at a certain point.
- Large file management (大容量ファイル管理): A mechanism for separate management of large binary assets (concept of Git LFS).
- Cherry-pick (チェリーピック): An operation to incorporate only specific commits into another branch.
- Defined check (定義済みチェック): The set of automated inspections required in repository settings (contents according to project definition).
- Merge commit method (統合コミット方式): A method of incorporating with a dedicated integration commit (concept of Merge Commit).
- Merge queue list (取り込み順一覧): A place where the order and current position of pending PRs are listed publicly (concept of Merge Queue).
- Split (分割): Dividing changes into multiple PRs by meaningful unit of purpose.
- Recovery (復旧): The act of safely returning unwanted incorporations to the previous state (centered on Revert).
- Branch update (ブランチ更新): A collective term for the operation of incorporating base branch (e.g., main) content into a PR branch (concept of UI operation).
- Protected branch (保護ブランチ): A branch with restrictions configured on direct push, force push, deletion, etc.
- Merge ready (マージ準備完了): The state where a PR is ready for incorporation (valid approval, required checks passed, no planned additional diffs, etc.).
- Label (ラベル): A mark indicating a PR's state, blocking factors, priority, and type (naming system).
- Rebase (リベース): An operation to rearrange the history of a working branch to follow the latest base.
- Remote main (リモートメイン): main on the server (the head to reference as the standard).
- Rolling key update (ローリング鍵更新): The procedure for invalidating a secret key and switching to a new key (concept of Token Rotation).
- Local main (ローカルメイン): The local main reference (name to distinguish from origin/main).
- Line-by-line history (行単位履歴): A record of who last updated each line and when (Blame).
- Latest commit (最新コミット): The most recently incorporated commit.
- Secret detection (秘密検知): A mechanism for automatically finding secret information mixed in.
- Secret information (秘密情報): Information that must not be made public, such as API keys and tokens.
- Secret contamination (秘密混入): The state where secret information is included in a PR/repository, or suspicion of this.
- Pull request (プルリクエスト): A unit of application for incorporating changes into main (container for description, discussion, and agreement).
- Main (メイン): Always-working "correct" branch (written as main in this document).
- Reset (リセット): An operation to switch the reference to the past (nature of potentially rewriting shared history).
- Revert (リバート): An operation to restore by adding a new commit that cancels incorporated changes (history is preserved).
- Revert PR (リバートPR): A PR containing only a Revert (explicitly note the target PR number or commit).
- History rewrite (履歴書き換え): A collective term for rewriting past commits on a shared branch.
- Continuous integration (連続統合): A mechanism for automatically running tests and static analysis (concept of CI).
- Priority (優先度): A concept representing the urgency and importance of processing a PR/Issue.
- Completion criteria (完了条件): Text defining the point at which a PR is considered finished (incorporation, related close, record update complete).

### 2.2 English Terms (A–Z | Definitions Only)

- .env: A file type containing environment variables (a container that may include Secrets).
- Approval: The state where a reviewer has judged incorporation as acceptable.
- Artifacts: Outputs that can be regenerated from source (build results, binaries, logs, caches, etc.).
- Authored Date: The date and time a commit was created (for reference).
- Blame: A display showing who last updated each line and when.
- blocked:ci-failed: Label indicating PR progress is stopped due to required check (CI) failure.
- blocked:needs-rebase: Label indicating PR progress is stopped due to needing base tracking (rebase).
- CHANGELOG: A human-readable summary record of important changes and compatibility notes.
- Cherry-pick: An operation to incorporate only specific commits into another branch.
- CI (Continuous Integration): A mechanism for automatically running tests and static analysis.
- CODEOWNERS: A mechanism for defining responsible reviewers per path.
- Commit / SHA: A unit of change / its unique identifier.
- Committed Date: The date and time a commit was incorporated into history.
- Definition of Done: A document defining the point at which a PR is considered complete.
- Definition of Ready: A document defining conditions under which an Issue/PR is considered ready to start.
- Draft Pull Request: A Pull Request in progress (used as a container for visibility of progress).
- Fast-Forward: A method of incorporating by advancing only the head reference when there is no branch.
- Feature branch: A short-term working branch branching from main.
- File History: A chronological list of commits related to a specific file.
- Force Push: A collective term for push operations that can rewrite shared history.
- Force-with-lease: A form of force push that is refused if the remote reference has advanced since it was last fetched.
- Git LFS: A mechanism for separate management of large files.
- History Rewrite: A collective term for rewriting past commits on a shared branch.
- Labels: A group of names indicating a PR's state, blocking factors, priority, and type.
- Latest Commit: The most recently incorporated commit.
- Linear History: A policy of keeping history in a straight line.
- local main: The local main reference (name to distinguish from origin/main).
- main: The always-working "correct" branch.
- Merge Commit: A method of incorporating with a dedicated integration commit.
- Merge Conflict: A state of diff conflict that cannot be automatically merged.
- Merge Queue: A place where the order and current position of pending PRs are listed publicly.
- origin/main: main on the server (the head to reference as the standard).
- PR Template: A template defining required items in the PR body.
- priority:high: Label indicating high priority.
- priority:normal: Label indicating normal priority.
- priority:low: Label indicating low priority.
- Protected Branch: A branch with restrictions configured on direct push, force push, deletion, etc.
- Pull Request (PR): A unit of application for incorporating changes into main.
- Rebase: History rearrangement to keep a working branch up to date with the latest main.
- Release / Tag: A marker/distribution unit pointing to the state at a certain point.
- Request Changes: The state where a reviewer is requesting changes.
- Required Status Checks: The collective name for automated inspections required in repository settings (contents according to project definition).
- Reset: Switching a reference (nature of potentially rewriting shared history — prohibited on shared branches).
- Revert: An operation to restore by adding a canceling commit (history is preserved).
- Revert Pull Request: A PR containing only Revert (explicitly note target PR number or commit).
- Rollback: Synonymous with Revert in this policy.
- Secret: Keys, tokens, etc. that must not be made public.
- Secret Scanning: A mechanism for automatically detecting Secret contamination.
- SLA (Service Level Agreement): An agreement on response time (specific values according to project definition).
- Small PR: The concept of submitting in small diff units (specific thresholds according to project definition).
- Squash Merge: A method of incorporating by combining multiple commits in a PR into one.
- Stale Approval: The state where an existing approval becomes invalid due to new commit addition or base incorporation.
- status:approved: Label indicating approved.
- status:changes-requested: Label indicating changes being addressed.
- status:draft: Label indicating in progress.
- status:merge-ready: Label indicating waiting for maintainer merge.
- status:ready-for-review: Label indicating review requested.
- Token Rotation: The procedure for invalidating a secret key and reissuing it.
- type:chore: Type label for housekeeping/mechanical changes.
- type:docs: Type label for documentation changes.
- type:feature: Type label for feature additions.
- type:fix: Type label for bug fixes.
- Update Branch: A collective term for the UI operation of incorporating base branch content into a PR branch.

---

## 3. Roles and Responsibilities

- Maintainer: Maintain main protection, organize review system, manage queue, perform Squash & Merge, lead Revert PR in case of incidents.
- Developer (Contributor): Work only via feature → PR (do not touch main). Prepare description, tests, and screenshots. Update own branch as needed.
- Reviewer: Review from perspectives of purpose, impact, safety (secrets/permissions), and tests. Reject below standard (huge PR, insufficient description, CI failure, secret contamination).

---

## 4. Operations Rules

### Required (MUST)

- main is read-only. Changes only via PR.
- Use Squash Merge by default. Adopting Non-FF (Merge Commit) requires presenting the reason and obtaining agreement. Fast-Forward is not used.
- The maintainer merges after review and the required local checks pass. CI starts after the source merge, as specified in §5-4; pre-merge CI status checks are not a merge requirement. Keep the PR requirement and the prohibitions on force push and branch deletion.
- Submit as small PRs (specific file count / diff line count thresholds according to project definition).
- Clearly state purpose / background / changes / impact / test steps in PR body, and link to Issue.
- Do not include secrets or artifacts in PRs.
- **CI is one workflow run.** After the source PR is merged, the developer uses that run to check the source and dependencies and build both OS artifacts. Required tools, checks and builds must all pass before artifacts are uploaded. Sign and inspect the artifacts, then merge one PR containing all inspection results. The record PR starts no CI. Windows signing resumes the same run for packaging.
- A branch name starts with the version its work will ship in: `v<version>`, or `v<version>-<keyword>` with the keyword naming the work. Lower case, digits, dots and hyphens only. No branch without a version. No prefix: a word such as `release` states a status, and a status does not belong in a name. Type is carried by the label, not by the branch name.

### Recommended (SHOULD)

- Feature branches are short-lived (delete after merge).
- Track the base by merging it into your branch, or by re-branching from `origin/main` (do not use GitHub's Update branch). Do not use rebase.
- Changes to this policy only via PR to this file (verbal exceptions prohibited).

### Commit / Push / PR (different purposes, different timing)

- **Commit**: frequent — regular local checkpoints that preserve work.
- **Push**: at a judged timing, for its own purpose (backing work up to the remote, making the branch visible); decoupled from raising a PR.
- **PR**: raised only when the assigned mission is complete and self-verified to a no-rework level — not per file, not rushed.
- Push and PR serve different purposes and are not performed at the same time.
- **A version's work stays on its versioned branch.** The source PR is merged before the release build; the inspection-record PR is raised once every inspection result is ready. These are the two PRs in §5-4.
- **Never press for a merge by claiming the work cannot proceed without one.** A documentation correction rides in the next version's PR.
- **Do not raise a follow-up PR.** Verify the change locally before handing it over.

### SLA (Response Time)

- According to project definition.

### Prohibited (NG)

- Raising pull requests in excess.
- Direct push / deletion / protection removal on main.
- Force push, in any spelling and on any branch — `-f`, `--force`, `--force-with-lease`, `--mirror`, a `+refspec`, or the equivalent through any API.
- Zip distribution / starting work from Zip (missing history).
- Huge PRs / PRs with insufficient description / merging with failed local checks / publishing an artifact whose CI run failed.
- Agreement / approval via DM (direct messages) (invalid outside PR).
- Reaching a blocked operation by another command, another API, or another tool. A block by a hook, a permission setting, or a branch protection is a statement of the requester's intent, not a description of the pass condition. When blocked, stop, and raise it on the PR or Issue before going further.
- Discarding work that exists only locally, or removing a shared reference, without prior agreement on the PR or Issue — `git reset --hard`, `git clean -fd`, `git checkout -f`, `git branch -D`, `git tag -d`, `git push --delete`, or the equivalent through any API. The list is illustrative, not exhaustive.
- Re-pointing a published tag or release. Per-release shipping inspection is bound to each artifact's hash (see [security-design.md](../dev-prog/security-design.md)).
- An incorporation path that bypasses approval or required checks (an administrator override merge, for example).

---

## 5. Operations Flow

Signal principle: Signals are only via PR comment mentions and label updates. DMs are invalid.

### 5-1. Planning to Ready (Issue)

- Maintainer: Organize Issue (purpose, background, acceptance criteria, impact, priority, deadline). Assign responsible developer and reviewer. Split into small PR units as needed. Signal: Write "ready to start" in Issue.
- Developer: Ask questions in Issue and confirm in writing. Create Draft PR and link to Issue.
- Reviewer: Confirm scope and prepare review perspectives (spec, security, tests).

Definition of Ready: Acceptance criteria confirmed / scope of impact visible / small PR unit / assignee, reviewer, and deadline determined.

### 5-2. PR Lifecycle

- status:draft (in progress) — Subject: Developer
  Organize PR body (purpose / background / changes / impact / test steps). Attach self-tests / screenshots. Keep to small PR.
  Signal: When ready → change to status:ready-for-review and mention reviewer.

- status:ready-for-review (review requested) — Subject: Reviewer
  Check: purpose, impact, no secrets/artifacts, local verification results, diff size.
  Judgment: No problems → approve (status:approved). Problems → specific feedback + status:changes-requested.
  Note: Do not add spec additions after request (diff expansion is a disqualification).

- status:changes-requested (addressing changes) — Subject: Developer
  Reply to feedback on PR and add minimum necessary fixes.
  Developer resolves blocked:needs-rebase (main tracking needed). A failed post-merge release CI is handled before publication under §5-4.
  Signal: When fixes complete → return to status:ready-for-review and re-request.

- status:approved (approved) — Subject: Developer → Maintainer
  Developer stops additional commits (approval is invalidated by additional diffs). Final check of body checklist.
  Handoff: Change to status:merge-ready and @maintainers explicitly in PR comment.

- status:merge-ready (waiting for maintainer merge) — Subject: Maintainer
  Leave acceptance comment, confirm approval is valid, local checks passed, no unnecessary additions, and Squash & Merge.
  Visibility: Processing order in fixed Merge Queue Issue.
  Do not use Auto-merge.

- Merged (incorporation complete) — Subject: Maintainer → Developer
  Maintainer: Close PR, delete branch (auto-setting possible).
  Developer: Close linked Issue. Update necessary documentation and CHANGELOG.

Definition of Done: Squash & Merge complete / Issue closed / related documentation updated / release impact recorded.

### 5-3. Handling Blocks (Common)

- blocked:needs-rebase: Developer catches up and resolves → return to status:ready-for-review.
- blocked:ci-failed: Developer records the failed release run and its cause on the PR. Do not publish until the failure is resolved and the required run succeeds. A retry after a failure is recorded as an additional run, not counted as the planned single successful run.
- Prohibited: Merging with stale approval, huge PRs, secret/artifact contamination.

---

### 5-4. Release process

**Publication.** A release is published when the shipped artifact is stored in `dist/<version>/` and is available at <https://github.com/Kozo2000/XOKSA/releases>. Publication is the final release step.

**CI execution.** The developer runs CI once to check and build the program when its source code or dependency libraries change. If neither changes, the developer does not rerun CI or rebuild the program.

**Process.** The developer confirms completion of each step before proceeding to the next step. Source and inspection-record changes are merged before publication.

| Step | Responsible role | Required work | Completion condition |
| :--- | :--- | :--- | :--- |
| 1. Source review | Maintainer | Review and merge the source PR. | Review and required local checks pass; the source is committed and merged. |
| 2. Checks and build | Developer | In one CI run, check the merged source and build the macOS and Windows artifacts. | Every required tool, check and build succeeds; artifact hashes and the source commit are recorded. |
| 3. Signing | Maintainer | Sign the artifacts. On macOS, notarize and staple. On Windows, sign the three executables, resume the same run to package without compilation, then sign the MSI. | All signatures verify; macOS notarization and stapling succeed; packaged executable payloads match the signed files. |
| 4. Shipping inspection | Developer | Inspect the signed artifacts under [the shipping inspection](../dev-prog/security-assessment.md) §C.1, including external scans, and prepare the SBOM. | All required results are available and bound to the artifact hashes. |
| 5. Inspection record | Maintainer | Review and merge one PR containing the §C.4 inspection record and SBOM. | The record is complete and merged. Do not run CI for this step. |
| 6. Artifact retention | Developer | Tag the commit containing the code and retain the artifact in `dist/<version>/`. | The artifact matches the inspected hashes; existing published tags remain unchanged. |
| 7. Publication | Maintainer | Attach the inspected artifact to the release. | The published artifact matches the inspected hashes and is available for download. |

**Checks and build requirements.**

- The developer builds shipping artifacts in CI from committed source in a clean working tree and fixed dependencies. Do not ship developer-machine builds.
- In the same run, check every workspace, including the settings app, with fmt, clippy, executable tests, audit, deny and unused-dependency checks. A missing tool or failed check or build stops artifact upload and publication. Local checks do not replace CI.
- Build the frontend, engine, settings app and desktop app. Build both sidecars before staging them and verify that their reported versions match the release version.
- Do not ship artifacts containing identifying build paths.
- Keep the scheduled advisory scan independent of release work.

**Signing and approval.**

- Do not store signing keys in CI.
- The repository owner's signing approval and its approval record are required before Windows packaging. Permit the owner to approve a run they started. Missing approval configuration does not count as approval.
- Do not keep a runner polling while approval is pending. Resume packaging within the same CI run; do not start a separate run or recompile.
- Sign the three executables extracted from the installer. Do not sign the standalone desktop executable in their place. Verify signatures and identity with the build payload before packaging, and verify that packaging leaves the signed executables unchanged.

**Source, artifact and record.**

- The developer records the artifact's build commit, branch and hashes in §C.4. Write the record only after all required inspection results are available.
- Before publication, the developer checks whether the program's source code or dependency libraries have changed since the artifact was built. If either changed, check and build the changed program in one CI run and inspect the new artifact through every stage in §C.1. If neither changed, publish the existing inspected artifact.
- Preserve the source commits and release branches named in inspection records. Do not delete release branches, repoint published tags or rewrite history.
- Use dated inspection facts in §C.4 headings. Keep mutable status and publication timestamps in the handover note. Do not require a publication date in §C.4 or create a PR solely to add that date. Preserve existing dated publication facts.
- When migrating without history, retain the source repository as a private archive and preserve its referenced commits and release branches. The first inspection record after migration identifies the repository to which it applies.

**Merge protection.** The maintainer verifies review and required local results before merging. Do not require pre-merge CI statuses when CI runs after the source merge. Preserve PR-only changes, stale-review dismissal, and force-push and deletion protections. Do not bypass protection.

**Inspection criteria.** Follow [the shipping inspection](../dev-prog/security-assessment.md) for inspection stages, measurement methods, SBOM preparation and release records.

## 6. Reading History and Dates (Basis for Judgment)

- Judgment is based on Committed Date (Authored Date is for reference).
- The effective point of a specification = the merge time of the PR.
- Last update to a file = File History, line-by-line = Blame.
- "Recent activity" = repository's Latest commit and PR list update order.
- Zip timestamps are invalid (no history).

---

## 7. Secrets, Artifacts, and Safety

- Do not commit Secrets and artifacts. PRs with suspicion of contamination are unconditionally rejected.
- In case of incident: Revert PR → Secret invalidation/reissue (Token Rotation) → Impact sharing → Prevention update.

---

## 8. Incident Response (Do-Not-Break Principle)

- Erroneous main incorporations are restored with a Revert PR (do not rewrite history).
- Hotfixes are also via PR. Squash by default; Non-FF requires presenting the reason and obtaining agreement. Direct push is not permitted.
- Exception: Only in the case of confirmed exposure of published Secrets, history removal (last resort) is exceptionally permitted after key invalidation and impact investigation.
- That exceptional history removal is carried out by a maintainer. Before it is carried out, the target commits, the affected branches, and the steps each person needs in order to re-sync are written on the PR or Issue.

---

## 9. Revisions to This Policy

- Changes are proposed only via PR to this file, with notification to all and maintainer approval required.
- Verbal exceptions and temporary operations are invalid.

---

<a id="ja"></a>

# xoksa GitHub運用ポリシー

> **言語同期:** 英語と日本語の記載はいずれも有効です。要件・制約・例外・コマンド挙動・運用ルールが片方の言語にだけ存在する場合でも、両方に適用されます。欠落している側は文書不備として更新対象です。

## 1. 基本規則

- 誤操作、履歴破壊、秘密情報の露出を防止する。
- 操作の実施者、内容、日時を追跡可能にする。
- 再現可能な作業手順を定める。
- 本規約にない例外や口頭の判断で工程を変更しない。
- 変更を必要な範囲に限定する。

---

## 2. 用語／略語（この文書での意味を固定）

### 2.1 日本語用語（五十音順｜定義のみ）

- 合図：進行の状態変化を相手に明確に伝える記述。ここでは PR のラベル更新と PR コメントでのメンション。
- 迂回：遮断された操作を、別のコマンド・API・ツールで達成しようとすること。
- 影響範囲：変更が及ぶ対象・箇所・関係者のまとまり（直接影響と副次影響を含む）。
- 受入基準：Issue／PR を完了とみなす条件の文章（期待動作・確認観点・除外範囲を含む）。
- 互換性注意：既存の利用やデータに調整・移行が必要となる点を明示する注意書き。
- 期日：その作業に対して合意された締切。
- 可視化：進行・順番・状態を誰でも同じ場所で確認できる形に置くこと（例：ラベル、キュー一覧）。
- 競合：自動では統合できない差分衝突の状態。
- コードオーナー：パスごとの責任レビュアーの定義。
- コミットID：変更の識別子（SHA）。
- コミット作成日時：コミットが作成された時刻（参考）。
- コミット取り込み日時：コミットが履歴に取り込まれた時刻（判断の拠り所）。
- 下書きプルリクエスト：作成途中の PR（進行の見える化に使う）。
- 小さなPR：小さな差分単位で提出するという考え方の呼称（具体閾値はプロジェクト定義に準じる）。
- 承認：レビュアーが取り込み可と判断した状態。
- 承認失効：新規コミットの追加やベース取り込みなどで既存承認が無効扱いになった状態。
- 修正要求：レビュアーが修正を求めている状態。
- 重要変更：ユーザー・運用・コスト・セキュリティ・互換性などに顕著な影響を与え得る変更の総称。
- 生成物：ソースから再生成できる出力物の総称（ビルド、バイナリ、ログ、キャッシュ等）。
- タグ／リリース：ある時点の状態を指し示すための印・配布単位。
- 大容量ファイル管理：大きなバイナリ資産を別管理する仕組み（Git LFS の概念）。
- チェリーピック：特定のコミットだけを別ブランチへ取り込む操作。
- 定義済みチェック：リポジトリ設定で必須に指定された自動検査の集合（内容はプロジェクト定義に準じる）。
- 統合コミット方式：統合専用のコミットを作って取り込む方式（Merge Commit の概念）。
- 取り込み順一覧：取り込み待ち PR の順序と現在地を一覧化して公開する場（Merge Queue の概念）。
- 分割：変更を目的ごとの意味単位で複数 PR に切り分けること。
- 復旧：望ましくない取り込みを安全に元の状態へ戻す行為（Revert 中心）。
- ブランチ更新：ベースブランチ（例：main）の内容を PR ブランチに取り込む操作の総称（UI 操作の概念）。
- 保護ブランチ：直 push・強制 push・削除などを設定で制限したブランチ。
- マージ準備完了：PR が取り込み可能な状態に整ったこと（承認有効・必須チェック成功・追加差分予定なし等）。
- ラベル：PR の状態・阻害要因・優先度・種別を示す印（名称体系）。
- リベース：作業ブランチの履歴を並べ替えて最新のベースに追随させる操作。
- リモートメイン：サーバ上の main（基準として参照する先端）。
- ローリング鍵更新：秘密鍵を失効させ新しい鍵へ切り替える手続き（Token Rotation の概念）。
- ローカルメイン：手元にある main の参照（origin/main と区別する呼称）。
- 行単位履歴：各行を最後に誰がいつ更新したかの記録（Blame）。
- 最新コミット：直近で取り込まれたコミット。
- 秘密検知：秘密情報の混入を自動で見つける仕組み。
- 秘密情報：API キー、トークン等の公開してはならない情報。
- 秘密混入：PR／リポジトリに秘密情報が含まれてしまう状態、またはその疑い。
- プルリクエスト：変更を main に取り込むための申請単位（説明・議論・合意の入れ物）。
- メイン：常に動作可能な"正"のブランチ（本文では main と表記）。
- リセット：参照先を過去に付け替える操作（共有履歴を書き換え得る性質）。
- リバート：取り込まれた変更を打ち消す新しいコミットで元に戻す操作（履歴は保持）。
- リバートPR：リバートだけを含む PR（対象の PR 番号やコミットを明記）。
- 履歴書き換え：共有ブランチの過去コミットを書き換える行為の総称。
- 連続統合：テストや静的解析などを自動で実行する仕組み（CI の概念）。
- 優先度：PR／Issue の処理の緊急度・重要度を表す概念。
- 完了条件：PR を終わったとみなす到達点の文章（取り込み・関連クローズ・記録更新の完了）。

### 2.2 英語用語（A–Z｜定義のみ）

- .env：環境変数を記載するファイル種別（Secret を含みうる入れ物）。
- Approval：レビュアーが取り込み可と判断した状態。
- Artifacts：ソースから再生成できる出力物（ビルド結果・バイナリ・ログ・キャッシュ等）。
- Authored Date：コミットが作成された日時（参考情報）。
- Blame：各行の最終更新者と更新時刻を示す表示。
- blocked:ci-failed：必須チェック（CI）失敗により PR の進行が止まっていることを示すラベル。
- blocked:needs-rebase：ベースへの追随（rebase）が必要で PR の進行が止まっていることを示すラベル。
- CHANGELOG：重要変更や互換性注意を人向けに要約した記録。
- Cherry-pick：特定コミットのみを別ブランチへ取り込む操作。
- CI (Continuous Integration)：テストや静的解析などを自動実行する仕組み。
- CODEOWNERS：パスごとの責任レビュアーを定義する仕組み。
- Commit / SHA：変更の単位／その一意識別子。
- Committed Date：コミットが履歴に取り込まれた日時。
- Definition of Done：PR を完了とみなす到達点を定義した文書。
- Definition of Ready：Issue／PR が着手可能とみなせる条件を定義した文書。
- Draft Pull Request：作成途中の Pull Request（進行の見える化に用いる器）。
- Fast-Forward：分岐がない場合に先端参照だけを進めて取り込む方式。
- Feature branch：main から分岐する短期の作業用ブランチ。
- File History：特定ファイルに関与したコミットの時系列。
- Force Push：共有履歴を書き換え得る push の総称。
- Force-with-lease：最後に取得して以降、リモート側の参照が進んでいた場合に拒否される force push の形式。
- Git LFS：大容量ファイルを別管理する仕組み。
- History Rewrite：共有ブランチの過去コミットを書き換える行為の総称。
- Labels：PR の状態・阻害要因・優先度・種別を示す名称群。
- Latest Commit：直近で取り込まれたコミット。
- Linear History：履歴を直線に保つ方針。
- local main：ローカルの main 参照（origin/main と区別する呼称）。
- main：常に動作可能な"正"のブランチ。
- Merge Commit：統合専用のコミットを作って取り込む方式。
- Merge Conflict：自動統合できない差分衝突の状態。
- Merge Queue：取り込み待ち PR の順序と現在地を一覧化して公開する場。
- origin/main：サーバ上の main（基準として参照する先端）。
- PR Template：PR 本文の必須項目を定めた雛形。
- priority:high：高い優先度を示すラベル。
- priority:normal：標準の優先度を示すラベル。
- priority:low：低い優先度を示すラベル。
- Protected Branch：直 push・強制 push・削除などを設定で制限したブランチ。
- Pull Request (PR)：変更を main に取り込むための申請単位。
- Rebase：作業ブランチを main 最新に追随させるための履歴並べ替え。
- Release / Tag：ある時点の状態を指し示す印・配布単位。
- Request Changes：レビュアーが修正を求めている状態。
- Required Status Checks：リポジトリ設定で必須に指定された自動検査の集合名（内容はプロジェクト定義に準じる）。
- Reset：参照先の付け替え（共有履歴では使用禁止の性質）。
- Revert：打ち消しコミットを追加して元に戻す操作（履歴は保持）。
- Revert Pull Request：Revert だけを含む PR（対象 PR 番号やコミットを明記）。
- Rollback：本ポリシーでは Revert と同義。
- Secret：公開不可の鍵・トークン等。
- Secret Scanning：Secret 混入を自動検知する仕組み。
- SLA (Service Level Agreement)：対応時間に関する取り決め（具体値はプロジェクト定義に準じる）。
- Small PR：小さな差分単位で提出するという考え方の呼称（具体閾値はプロジェクト定義に準じる）。
- Squash Merge：PR 内の複数コミットを 1 つにまとめて取り込む方式。
- Stale Approval：新規コミット追加やベース取り込みで既存承認が無効扱いになった状態。
- status:approved：承認済みを示すラベル。
- status:changes-requested：修正対応中を示すラベル。
- status:draft：作成中を示すラベル。
- status:merge-ready：管理者マージ待ちを示すラベル。
- status:ready-for-review：レビュー依頼中を示すラベル。
- Token Rotation：秘密鍵の失効と再発行の手続き。
- type:chore：雑務・機械的変更の種別ラベル。
- type:docs：文書変更の種別ラベル。
- type:feature：機能追加の種別ラベル。
- type:fix：不具合修正の種別ラベル。
- Update Branch：ベースブランチ（例：main）の内容を PR ブランチに取り込む UI 操作の総称。

---

## 3. 役割と責務

- 管理者（Maintainer）：main の保護維持、レビュー体制整備、キュー管理、Squash & Merge 実施、事故時の Revert PR 主導。
- 開発者（Contributor）：feature → PR でのみ作業（main は触らない）。説明・テスト・スクショを揃える。必要に応じ自分ブランチを最新化。
- レビュアー（Reviewer）：目的・影響・安全（秘密/権限）・テスト観点でレビュー。基準未満（巨大 PR、説明不足、CI 失敗、秘密混入）は差し戻し。

---

## 4. 運用ルール

### 必須（MUST）

- main は読み取り専用。変更は PR 経由のみ。
- 取り込みは Squash Merge を原則とする。Non-FF（Merge Commit）を採用する際はその理由を提示し合意を得ること。Fast-Forward は使用しない。
- 管理者はレビューと必須のローカル検査の合格を確認してマージする。CI は §5-4 に従ってソースのマージ後に起動し、マージ前の CI ステータスチェックを必須条件にしない。PR 経由の変更、強制 push 禁止、ブランチ削除禁止の保護は維持する。
- 小さな PR で提出する（具体的なファイル数・差分行数の閾値はプロジェクト定義に準じる）。
- PR 本文に 目的／背景／変更点／影響／テスト手順 を明記し、Issue へ紐付け。
- 秘密情報・生成物は PR に含めない。
- **CI は1つの実行である。** 開発者は、ソースのPRをマージした後、その1回でソースと依存を検査し、両OSの現物をビルドする。必須ツール・検査・ビルドがすべて揃って成功した場合だけ成果物を出す。署名・現物検査の後、検査記録のPRを1本マージする。記録のPRではCIを起動しない。Windowsの署名後の梱包も同じ実行を再開する。
- ブランチ名は、その作業を載せる版数から始める（`v<版数>` または `v<版数>-<キーワード>`。キーワードは作業内容）。小文字・数字・ドット・ハイフンのみ。版数の無いブランチは作らない。接頭辞は付けない——`release` のような語は状態を表すものであり、状態を名前に固定しない。種別はラベルで示す。

### 推奨（SHOULD）

- feature ブランチは短命（マージ後は削除）。
- ベースへの追随は、ベースを自ブランチに取り込むか、`origin/main` から切り直して行う（GitHub の Update branch は使用しない）。rebase は用いない。
- 本ポリシーの変更は本ファイルへの PR のみ（口頭例外は禁止）。

### コミット／プッシュ／PR（目的が違うので、タイミングも分ける）

- **コミット**：頻繁でよい。作業を保全する定期的なローカルの区切り。
- **プッシュ**：それ自体の目的（作業のリモート退避・ブランチの可視化）のため、頃合いを見て行う。PR を出すこととは切り離す。
- **PR**：与えられたミッションが完了し、手戻りが無いと自己確認できたレベルで初めて出す。ファイル単位・拙速に出さない。
- プッシュと PR は目的が異なり、同じタイミングで行わない。
- **版の作業は版数を付けたブランチに置く。** ソースの PR は出荷ビルドより前にマージし、検査記録の PR は全検査結果が揃ってから 1 本出す。§5-4 の PR 2 本はこの 2 つを指す。
- **「PR を出さないと先に進めない」と言って急かさない。** 文書の修正は、次の版の PR に同梱すれば足りる。
- **後追いの PR を作らない。** ローカルで検証してから渡す。

### SLA（対応時間）

- プロジェクト定義に準じる。

### 禁止（NG）

- PR の乱発。
- main 直 push／削除／保護解除。
- force push（綴りを問わず、ブランチを問わず）。`-f`・`--force`・`--force-with-lease`・`--mirror`・`+refspec`、および任意の API による同等の操作。
- Zip 配布・Zip から作業開始（履歴欠落）。
- 巨大 PR・説明不足 PR・ローカル検査失敗のままのマージ・CI 失敗の現物の公開。
- DM（ダイレクトメッセージ）での合意・承認（PR 外は無効）。
- 遮断された操作を、別のコマンド・別の API・別のツールで達成しようとすること。フック・権限設定・ブランチ保護による遮断は、通過条件の記述ではなく依頼者の意図の表明として扱う。遮断に当たったら止まり、PR または Issue で相談してから先へ進む。
- ローカルにしか存在しない作業の破棄、または共有参照の削除を、PR／Issue での事前合意なしに行うこと（`git reset --hard`・`git clean -fd`・`git checkout -f`・`git branch -D`・`git tag -d`・`git push --delete`、および任意の API による同等の操作）。列挙は例示であり、網羅ではない。
- 公開済みのタグ／リリースの付け替え。リリースごとの出荷検査は各現物のハッシュに紐づく（[security-design.md](../dev-prog/security-design.md) 参照）。
- 承認・必須チェックを迂回する取り込み（管理者権限による強制マージ等）。

---

## 5. 運用フロー

合図の原則：合図は PR コメントのメンション と ラベル更新 だけ。DM は無効。

### 5-1. 企画〜着手可（Issue）

- 管理者：Issue を整備（目的・背景・受入基準・影響・優先度・期日）。担当開発者とレビュアーを指名。必要に応じ小 PR 単位に分割。合図：Issue に「開始可」を明記。
- 開発者：不明点は Issue で質問し文章で確定。Draft PR を作成して Issue へリンク。
- レビュアー：担当範囲を確認し、レビュー観点（仕様・セキュリティ・テスト）を準備。

Definition of Ready：受入基準が確定／影響範囲が見える／小 PR 単位／担当・レビュアー・期日が決定。

### 5-2. PR ライフサイクル

- status:draft（作成中） — 主語：開発者
  PR 本文を整備（目的／背景／変更点／影響／テスト手順）。自己テスト・スクショを添付。小 PR に収める。
  合図：準備完了 → status:ready-for-review に変更し、レビュアーをメンション。

- status:ready-for-review（レビュー依頼中） — 主語：レビュアー
  確認：目的・影響・秘密/生成物なし・ローカルの検証結果・差分サイズ。
  判定：問題なし→承認（status:approved）。問題あり→具体指摘＋ status:changes-requested。
  注意：依頼後に仕様追加をしない（差分膨張は失格）。

- status:changes-requested（修正対応中） — 主語：開発者
  指摘へ PR 上で返信し、必要最小の修正を追加。
  blocked:needs-rebase（追随必要）は開発者が解消する。マージ後の出荷 CI の失敗は §5-4 に従い、公開前に解消する。
  合図：修正完了 → status:ready-for-review に戻して再依頼。

- status:approved（承認済み） — 主語：開発者 → 管理者
  開発者は追加コミットを止める（承認は追加差分で失効）。本文チェック項目を最終確認。
  引き渡し：status:merge-ready に変更し、PR コメントで @maintainers を明示。

- status:merge-ready（管理者マージ待ち） — 主語：管理者
  受領コメントを残し、承認有効・ローカル検査合格・不要追加なしを確認して Squash & Merge。
  可視化：処理順は固定の Merge Queue Issue に。
  Auto-merge は使用しない。

- Merged（取り込み完了） — 主語：管理者 → 開発者
  管理者：PR を閉じ、ブランチ削除（自動設定可）。
  開発者：紐づく Issue をクローズ。必要ドキュメントや CHANGELOG を更新。

Definition of Done：Squash & Merge 済み／Issue クローズ／関連ドキュメント更新／リリース影響を記録。

### 5-3. ブロック時の扱い（共通）

- blocked:needs-rebase：開発者が追随して解消 → status:ready-for-review へ復帰。
- blocked:ci-failed：開発者は失敗した出荷 CI の実行番号と原因を PR に記録する。失敗を解消し、必要な実行が成功するまで公開しない。失敗による再実行は追加の実行として記録し、予定した正常時の 1 回に数え込まない。
- 禁止：古い承認のままのマージ（stale approval）、巨大 PR、秘密・生成物の混入。

---

### 5-4. リリース工程

**公開の定義。** 出荷する現物を `dist/<版数>/` に保管し、<https://github.com/Kozo2000/XOKSA/releases> で入手できる状態を公開とする。公開をリリースの最終工程とする。

**CIの実行条件。** 開発者は、プログラムのソースコードまたは依存ライブラリを変更したとき、CIを1回実行して検査とビルドを行う。ソースコードと依存ライブラリを変更していなければ、CIやビルドをやり直さない。

**工程。** 開発者は各工程の完了を確認してから次の工程へ進む。ソースと検査記録の変更は、公開前にマージする。

| 工程 | 担当者 | 実施内容 | 完了条件 |
| :--- | :--- | :--- | :--- |
| 1. ソースの審査 | 管理者 | ソースのPRをレビューしてマージする。 | レビューと必須のローカル検査が合格し、ソースがコミット・マージされている。 |
| 2. 検査とビルド | 開発者 | CIの1回の実行で、マージ済みソースを検査し、macOSとWindowsの現物をビルドする。 | 必須ツール・検査・ビルドがすべて成功し、現物のハッシュとソースコミットが記録されている。 |
| 3. 署名 | 管理者 | 現物に署名する。macOSは公証とstapleを行う。Windowsは実行ファイル3本に署名し、同じ実行を再開して再コンパイルせずに梱包し、そのMSIに署名する。 | 署名が有効で、macOSの公証とstapleが完了し、梱包された実行ファイルが署名済み現物と一致している。 |
| 4. 出荷検査 | 開発者 | [出荷検査](../dev-prog/security-assessment.md) §C.1 に従い、外部スキャンを含む署名済み現物の検査とSBOMの作成を行う。 | 必須の検査結果がすべて揃い、現物のハッシュと対応している。 |
| 5. 検査記録 | 管理者 | §C.4の検査記録とSBOMを含むPRを1本レビューしてマージする。 | 記録が完成し、マージされている。この工程でCIは実行しない。 |
| 6. 現物の保管 | 開発者 | 実体を含むコミットにタグを付け、現物を `dist/<版数>/` に保管する。 | 現物が検査済みハッシュと一致し、公開済みタグを変更していない。 |
| 7. 公開 | 管理者 | 検査済み現物をリリースに添付する。 | 公開された現物が検査済みハッシュと一致し、ダウンロードできる。 |

**検査とビルドの要件。**

- 開発者は、未コミットの変更がない作業ツリーで、コミット済みソースと固定済み依存からCIで出荷現物をビルドする。開発機でビルドした現物は出荷しない。
- 同じ実行内で、設定アプリを含む全workspaceにfmt・clippy・実行可能なtest・audit・deny・未使用依存の検査を行う。必須ツールの欠落、検査またはビルドの失敗があれば、現物のアップロードと公開を停止する。ローカル検査でCIを代替しない。
- フロントエンド、エンジン、設定アプリ、デスクトップアプリをビルドする。両sidecarをビルドしてから配置し、報告する版数とリリースの版数が一致することを確認する。
- 個人を特定するビルドパスを含む現物は出荷しない。
- 定時の勧告スキャンは出荷作業から独立して維持する。

**署名と承認。**

- 署名キーをCIに保存しない。
- Windowsの梱包前に、リポジトリのオーナー本人の署名承認と承認記録を必須とする。オーナー自身が開始した実行の承認を認める。承認設定の欠落を承認として扱わない。
- 承認待ちの間、ランナーをポーリングのために動かし続けない。同じCI実行内で梱包を再開し、別の実行の開始や再コンパイルを行わない。
- インストーラから取り出した実行ファイル3本を署名対象とし、単体ビルドのデスクトップ実行ファイルを代用しない。梱包前に署名とビルドされた現物との一致を検証し、梱包後も署名済み実行ファイルが変わっていないことを検証する。

**ソース・現物・記録の対応。**

- 開発者は、現物のビルド元コミット・ブランチ・ハッシュを§C.4に記録する。記録は、必須の検査結果がすべて揃ってから書く。
- 開発者は公開前に、現物のビルド後にプログラムのソースコードまたは依存ライブラリを変更していないか確認する。変更していれば、変更後のプログラムにCIを1回実行して検査とビルドを行い、新しい現物に§C.1の全段階の検査を行う。変更していなければ、既存の検査済み現物を公開する。
- 検査記録に記載したソースコミットとreleaseブランチを保持する。releaseブランチの削除、公開済みタグの付け替え、履歴の書き換えを行わない。
- §C.4の見出しには日付付きの検査事実を記載する。変動する状態と公開日時は引継ぎ書に記載する。§C.4への公開日の追記を必須とせず、その追記だけのPRを作成しない。既存の日付付き公開事実は保持する。
- 履歴を引き継がずに移行する場合、移行元を非公開のアーカイブとして保持し、参照されるコミットとreleaseブランチを削除しない。移行後の最初の検査記録に、対象リポジトリを明記する。

**マージの保護。** 管理者は、レビューと必須のローカル検査結果を確認してからマージする。CIをソースのマージ後に実行する場合、マージ前のCIステータスを必須条件としない。PR経由の変更、追加差分による承認失効、強制pushと削除を禁止する保護を維持する。保護を迂回しない。

**検査基準。** 検査の段階、測定方法、SBOMの作成方法、リリースの記録は[出荷検査](../dev-prog/security-assessment.md)に従う。

## 6. 履歴と日付の見方（判断の拠り所）

- 判断は Committed Date を正（Authored Date は参考）。
- 仕様の発効時点＝PR のマージ時刻。
- ファイルの最終更新＝ File History、行単位＝ Blame。
- 「最近の動き」＝リポジトリの Latest commit と PR 一覧の更新順。
- Zip のタイムスタンプは無効（履歴がないため）。

---

## 7. 秘密・生成物・安全

- Secret と生成物はコミットしない。混入の疑いがある PR は無条件差し戻し。
- 事故時：Revert PR → 秘密無効化/再発行（Token Rotation）→ 影響共有 → 再発防止の更新。

---

## 8. 事故対応（壊さない原則）

- main の誤取り込みは Revert PR で戻す（履歴は書き換えない）。
- Hotfix も PR 経由。Squash を原則とし、Non-FF はその理由を提示し合意を得ること。直 push は許容しない。
- 例外：公開済み Secret 混入が確認された場合に限り、鍵失効・影響調査のうえ、履歴除去（最終手段）を例外として許容。
- この例外的な履歴除去は管理者が実施する。実施前に、対象コミット・影響を受けるブランチ・各自が再同期するための手順を PR または Issue に明記する。

---

## 9. 本ポリシーの改定

- 変更は本ファイルへの PR でのみ提案し、全員通知と管理者承認を必須。
- 口頭例外・一時運用は無効。
