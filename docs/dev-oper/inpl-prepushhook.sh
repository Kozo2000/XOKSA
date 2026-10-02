# 1) フック置き場
mkdir -p .git/hooks

# 2) pre-push フック作成（origin の main だけを拒否）
cat > .git/hooks/pre-push <<'SH'
#!/usr/bin/env bash
set -euo pipefail

remote_name="$1"   # 例: origin
remote_url="$2"

BLOCK_REMOTE="origin"
BLOCK_BRANCH="refs/heads/main"   # 対象ブランチ

# origin 以外への push は対象外
[[ "$remote_name" == "$BLOCK_REMOTE" ]] || exit 0

# 受け渡される参照群を精査
while read -r local_ref local_sha remote_ref remote_sha; do
  # ブランチ push のとき remote_ref は refs/heads/<name>
  if [[ "$remote_ref" == "$BLOCK_BRANCH" ]]; then
    echo "❌ push blocked: '$BLOCK_REMOTE $BLOCK_BRANCH' への push はプロジェクト方針で禁止です。"
    echo "   feature ブランチ + Pull Request を使ってください。（--no-verify は使用禁止）"
    exit 1
  fi
done

exit 0
SH

# 3) 実行権付与
chmod +x .git/hooks/pre-push
