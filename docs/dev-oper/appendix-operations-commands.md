Language sync
English and Japanese descriptions are both authoritative. If a requirement, restriction, exception, command behavior, or operational rule appears in only one language, treat it as applying to both; the missing side is a documentation defect to be updated.

言語同期
英語と日本語の記載はいずれも有効です。要件・制約・例外・コマンド挙動・運用ルールが片方の言語にだけ存在する場合でも、両方に適用されます。欠落している側は文書不備として更新対象です。

Common Prerequisites
共通前提
＃対象者：全員 / Target: Everyone
＃作業内容：リモートの最新取得と pull 挙動の固定 / Fetch latest remote and fix pull behavior
＃注意事項：git config pull.ff only は必須／HTTPSよりSSH推奨／local main は checkout しない
＃Notes: git config pull.ff only is required / SSH preferred over HTTPS / do not checkout local main

gh auth status
＃対象：全員 / Target: Everyone
＃GH CLI のログイン確認（未ログインなら gh auth login）/ Verify GH CLI login (run gh auth login if not logged in)

git remote -v
＃対象：全員 / Target: Everyone
＃origin の URL を確認（意図したリポジトリか／SSH 推奨）/ Verify origin URL (correct repo / SSH preferred)

git config pull.ff only
＃対象：全員 / Target: Everyone
＃pull を Fast-Forward のみに固定（不要なマージコミット防止）/ Fix pull to Fast-Forward only (prevent unnecessary merge commits)
＃注意：個人設定が違っていたら必ず合わせる / Note: align if personal setting differs

git fetch --prune origin
＃対象：全員 / Target: Everyone
＃リモートの最新を取得／不要な追跡ブランチを整理 / Fetch latest from remote / clean up stale tracking branches

git rev-parse --verify origin/main
＃対象：全員 / Target: Everyone
＃origin/main が取得できているか検証（無ければ fetch を再確認）/ Verify origin/main is available (re-check fetch if absent)

5-1 Planning to Ready (Issue)
5-1 企画〜着手可（Issue）
＃対象者：開発者 / Target: Developer
＃作業内容：feature ブランチ作成 → Draft PR 作成 → ラベル付与／Issue 連携
＃Work: Create feature branch → create Draft PR → assign labels / link Issue
＃注意事項：ブランチ名は英数とハイフンのみ／秘密・生成物を含めない／local main は使わない
＃Notes: Branch name: alphanumeric and hyphens only / no secrets or artifacts / do not use local main

git switch -c feature/<issue-短名> origin/main
＃対象：開発者 / Target: Developer
＃origin/main を起点に作業ブランチを新規作成して切替 / Create new working branch from origin/main and switch to it

git push -u origin feature/<issue-短名>
＃対象：開発者 / Target: Developer
＃作業ブランチをリモートへ初回公開し追跡設定（-u）を結ぶ / Publish working branch to remote for the first time and set tracking (-u)
＃注意：同名ブランチが既にある場合は中断して確認 / Note: stop and verify if a branch with the same name already exists

gh pr create --draft --base main --head feature/<issue-短名> --fill
＃対象：開発者 / Target: Developer
＃main 向けの Draft PR を作成（PR テンプレ自動埋め）/ Create Draft PR targeting main (auto-fill PR template)

gh pr edit --add-label "status:draft"
＃対象：開発者 / Target: Developer
＃状態ラベル：作成中 / Status label: in progress

gh pr edit --add-label "type:<feature|fix|docs|chore>"
＃対象：開発者 / Target: Developer
＃種別ラベルを付与（いずれか一つ）/ Assign type label (one of the above)

gh pr edit --add-label "priority:<high|normal|low>"
＃対象：開発者 / Target: Developer
＃優先度ラベルを付与（いずれか一つ）/ Assign priority label (one of the above)

gh pr comment -b "links: resolves #<issue-number>"
＃対象：開発者 / Target: Developer
＃対象 Issue とリンク（マージ時に自動クローズ）/ Link to target Issue (auto-close on merge)

5-2 PR Lifecycle — draft → ready-for-review
5-2 PRライフサイクル — draft → ready-for-review
＃対象者：開発者 / Target: Developer
＃作業内容：差分作成 → PR をレビュー依頼状態へ / Work: Create diff → move PR to review-requested state
＃注意事項：最小差分でコミット／自己テストの証跡（スクショ等）を添付 / Notes: commit minimal diff / attach self-test evidence (screenshots, etc.)

git add -p
＃対象：開発者 / Target: Developer
＃必要最小の差分だけをステージ / Stage only the minimum necessary diff

git commit -m "<type>: <要約>"
＃対象：開発者 / Target: Developer
＃意味単位で履歴化（type は feature/fix/docs/chore）/ Create history in meaningful units (type: feature/fix/docs/chore)

git push
＃対象：開発者 / Target: Developer
＃PR に反映 / Reflect in PR

gh pr ready
＃対象：開発者 / Target: Developer
＃Draft を通常 PR へ切替 / Switch Draft to regular PR

gh pr edit --remove-label "status:draft"
＃対象：開発者 / Target: Developer
＃Draft ラベルを外す / Remove Draft label

gh pr edit --add-label "status:ready-for-review"
＃対象：開発者 / Target: Developer
＃レビュー依頼中ラベルを付与 / Assign review-requested label

gh pr edit --add-reviewer <github-id-1> --add-reviewer <github-id-2>
＃対象：開発者 / Target: Developer
＃レビュアーを指名 / Assign reviewers

gh pr comment -b "@<reviewer1> @<reviewer2> レビューお願いします"
＃対象：開発者 / Target: Developer
＃メンションで依頼を明示 / Explicitly request via mention

5-2 PR Lifecycle — ready-for-review (judgment)
5-2 PRライフサイクル — ready-for-review（判定）
＃対象者：レビュアー / Target: Reviewer
＃作業内容：承認 or 修正要求の判定 / Work: Approve or request changes
＃注意事項：CI 成功／秘密・生成物なし／差分過大は差し戻し / Notes: CI must pass / no secrets or artifacts / reject oversized diffs

gh pr checks --watch
＃対象：レビュアー / Target: Reviewer
＃必須チェック（CI 等）の状況を確認 / Check status of required checks (CI, etc.)

gh pr review --approve -b "LGTM"
＃対象：レビュアー / Target: Reviewer
＃承認を付与 / Grant approval

gh pr edit --add-label "status:approved"
＃対象：レビュアー / Target: Reviewer
＃承認済みラベルを付与 / Assign approved label

gh pr edit --remove-label "status:ready-for-review"
＃対象：レビュアー / Target: Reviewer
＃レビュー依頼中ラベルを除去 / Remove review-requested label

gh pr review --request-changes -b "<修正理由>"
＃対象：レビュアー / Target: Reviewer
＃（代替）修正要求を記録 / (Alternative) Record request for changes

gh pr edit --add-label "status:changes-requested"
＃対象：レビュアー / Target: Reviewer
＃修正対応中ラベルを付与 / Assign changes-requested label

gh pr edit --remove-label "status:ready-for-review"
＃対象：レビュアー / Target: Reviewer
＃レビュー依頼中ラベルを除去 / Remove review-requested label

5-2 PR Lifecycle — changes-requested response
5-2 PRライフサイクル — changes-requested 対応
＃対象者：開発者 / Target: Developer
＃作業内容：指摘の最小修正を反映し再依頼 / Work: Apply minimum fixes for feedback and re-request review
＃注意事項：不要な仕様追加を混ぜない／差分は小さく / Notes: do not mix in unnecessary spec additions / keep diff small

git status
＃対象：開発者 / Target: Developer
＃作業ツリーとステージの状態確認 / Check working tree and staging state

git add -p
＃対象：開発者 / Target: Developer
＃修正分のみステージ / Stage only the fix

git commit -m "fix: <内容>"
＃対象：開発者 / Target: Developer
＃修正意図を明確化 / Clarify fix intent

git push
＃対象：開発者 / Target: Developer
＃PR へ反映 / Reflect in PR

gh pr edit --remove-label "status:changes-requested"
＃対象：開発者 / Target: Developer
＃修正対応中ラベルを外す / Remove changes-requested label

gh pr edit --add-label "status:ready-for-review"
＃対象：開発者 / Target: Developer
＃再レビュー依頼中ラベルを付与 / Assign re-review-requested label

gh pr comment -b "指摘対応しました。再レビューお願いします"
＃対象：開発者 / Target: Developer
＃合図をコメントで残す / Leave signal as a comment

5-2 PR Lifecycle — approved → merge-ready handoff
5-2 PRライフサイクル — approved → merge-ready 引き渡し
＃対象者：開発者 / Target: Developer
＃作業内容：管理者のマージ待ちへ移行 / Work: Move to waiting for maintainer merge
＃注意事項：承認失効させる追加コミットを止める／CI 緑を維持 / Notes: stop adding commits that would invalidate approval / keep CI green

gh pr edit --add-label "status:merge-ready"
＃対象：開発者 / Target: Developer
＃管理者マージ待ちラベルを付与 / Assign merge-ready label

gh pr edit --remove-label "status:approved"
＃対象：開発者 / Target: Developer
＃承認済みラベルを外す / Remove approved label

gh pr comment -b "@maintainers merge ready"
＃対象：開発者 / Target: Developer
＃管理者へ引き渡し合図 / Signal handoff to maintainer

5-2 PR Lifecycle — merge-ready processing (merge)
5-2 PRライフサイクル — merge-ready の処理（マージ）
＃対象者：管理者 / Target: Maintainer
＃作業内容：Squash & Merge／処理順の公開／ブランチ削除 / Work: Squash & Merge / publish processing order / delete branch
＃注意事項：Auto-merge は使わない／承認・CI の有効性を再確認 / Notes: do not use Auto-merge / re-verify approval and CI validity

gh issue comment <queue-issue-number> -b "queued: #<pr-number>"
＃対象：管理者 / Target: Maintainer
＃マージ順をキュー Issue に記録（可視化）/ Record merge order in queue Issue (for visibility)

gh pr merge --squash --delete-branch <pr-number>
＃対象：管理者 / Target: Maintainer
＃Squash で取り込み、PR ブランチを削除 / Merge with Squash and delete PR branch

gh issue comment <queue-issue-number> -b "merged: #<pr-number>"
＃対象：管理者 / Target: Maintainer
＃完了をキュー Issue に記録 / Record completion in queue Issue

5-2 PR Lifecycle — post-merge tasks
5-2 PRライフサイクル — Merged 後処理
＃対象者：管理者・開発者 / Target: Maintainer / Developer
＃作業内容：関連 Issue クローズ／必要ドキュメント更新 / Work: Close related Issue / update necessary documentation
＃注意事項：CHANGELOG 運用がある場合のみ更新 PR を切る / Notes: create update PR only if CHANGELOG policy applies

gh issue close <issue-number>
＃対象：管理者 / Target: Maintainer
＃関連 Issue をクローズ（PR で自動クローズ済みなら不要）/ Close related Issue (skip if auto-closed by PR)

git fetch origin
＃対象：開発者 / Target: Developer
＃最新を取得 / Fetch latest

git switch -c docs/changelog origin/main
＃対象：開発者 / Target: Developer
＃ドキュメント更新用の短期ブランチ作成 / Create short-lived branch for documentation update

git add CHANGELOG.md
＃対象：開発者 / Target: Developer
＃変更履歴の更新をステージ / Stage changelog update

git commit -m "docs(changelog): update for #<pr-number>"
＃対象：開発者 / Target: Developer
＃変更履歴更新のコミット / Commit changelog update

git push -u origin docs/changelog
＃対象：開発者 / Target: Developer
＃ブランチを公開し追跡設定 / Publish branch and set tracking

gh pr create --base main --head docs/changelog --title "Update CHANGELOG" --fill
＃対象：開発者 / Target: Developer
＃CHANGELOG 更新の PR を作成 / Create PR for CHANGELOG update

5-3 Blocked handling — blocked:needs-rebase
5-3 ブロック時の扱い — blocked:needs-rebase
＃対象者：開発者 / Target: Developer
＃作業内容：main 進行に追随し競合を解消 / Work: Catch up with main progress and resolve conflicts
＃注意事項：push は --force-with-lease を必ず使用（他者更新保護）/ Notes: always use --force-with-lease for push (protect others' updates)

git fetch origin
＃対象：開発者 / Target: Developer
＃最新 main を取得 / Fetch latest main

git rebase origin/main
＃対象：開発者 / Target: Developer
＃自ブランチを最新 main 上に並べ替え / Rebase own branch onto latest main

git add <解消したファイル>
＃対象：開発者 / Target: Developer
＃競合解消結果を登録 / Stage conflict resolution result

git rebase --continue
＃対象：開発者 / Target: Developer
＃リベースを継続 / Continue rebase

git push --force-with-lease
＃対象：開発者 / Target: Developer
＃安全確認付きで履歴を更新 / Update history with safety check

gh pr edit --remove-label "blocked:needs-rebase"
＃対象：開発者 / Target: Developer
＃ブロック解除を明示 / Explicitly unblock

5-3 Blocked handling — blocked:ci-failed
5-3 ブロック時の扱い — blocked:ci-failed
＃対象者：開発者 / Target: Developer
＃作業内容：失敗ジョブの原因特定と修正 / Work: Identify cause of failed job and fix
＃注意事項：失敗理由と対策を PR コメントに残す / Notes: leave failure reason and countermeasure in PR comment

gh pr checks
＃対象：開発者 / Target: Developer
＃現在のチェック状況を一覧 / List current check status

gh run list --branch feature/<issue-短名>
＃対象：開発者 / Target: Developer
＃実行履歴を一覧 / List execution history

gh run view <run-id> --log
＃対象：開発者 / Target: Developer
＃失敗ジョブの詳細ログを確認 / Check detailed log of failed job

git add -p
＃対象：開発者 / Target: Developer
＃修正分のみステージ / Stage only the fix

git commit -m "ci: fix <内容>"
＃対象：開発者 / Target: Developer
＃CI 修正コミットを作成 / Create CI fix commit

git push
＃対象：開発者 / Target: Developer
＃修正を送信し CI を再実行 / Send fix and re-run CI

gh pr edit --remove-label "blocked:ci-failed"
＃対象：開発者 / Target: Developer
＃成功後にブロック解除を明示 / Explicitly unblock after success

Incident Response — Revert PR
事故対応 — Revert PR
＃対象者：管理者（または指示された開発者）/ Target: Maintainer (or designated developer)
＃作業内容：main の誤取り込みを履歴を書き換えずに取り消す / Work: Undo erroneous merge to main without rewriting history
＃注意事項：複数コミットなら対象を特定／秘密露出時は鍵の失効・再発行を並行
＃Notes: identify target if multiple commits / invalidate and reissue keys in parallel if secrets are exposed

git fetch origin
＃対象：管理者 / Target: Maintainer
＃最新 main を取得 / Fetch latest main

git switch -c revert/<short-sha> origin/main
＃対象：管理者 / Target: Maintainer
＃取り消し用ブランチを main 上に作成 / Create revert branch on main

git revert <short-sha>
＃対象：管理者 / Target: Maintainer
＃指定コミットを打ち消すコミットを作成 / Create commit that undoes the specified commit

git push -u origin revert/<short-sha>
＃対象：管理者 / Target: Maintainer
＃リモートへ公開し追跡設定 / Publish to remote and set tracking

gh pr create --base main --head revert/<short-sha> --title "Revert: <元タイトル>" --body "Reverts #<pr-number>"
＃対象：管理者 / Target: Maintainer
＃Revert 専用 PR を作成 / Create Revert-only PR

gh pr edit --add-label "type:fix"
＃対象：管理者 / Target: Maintainer
＃種別ラベル / Type label

gh pr edit --add-label "priority:high"
＃対象：管理者 / Target: Maintainer
＃優先度ラベル（高）/ Priority label (high)
