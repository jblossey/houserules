#!/usr/bin/env bash
# Rewrite every commit's author and committer email from the personal address to the GitHub
# noreply address, remap the commit shas the tree pins, and force-push under a temporarily
# disabled ruleset. HR-093, owner ruling 2026-09-11. Run from anywhere; it cds into the repo.
# Preconditions: PR #13 merged, every worktree clean, `gh` authenticated as the repository admin.
set -u
REPO="$HOME/projects/houserules"
OLD='Jannis@blossey.eu'
NEW='57671651+jblossey@users.noreply.github.com'
MAP="$REPO/.git/email-rewrite-commit-map.txt"
STAMP=2026-09-11
RULESET_MAIN=22224803
RULESET_TAGS=22902575
export OLD NEW MAP FILTER_BRANCH_SQUELCH_WARNING=1

cd "$REPO" || exit 1
git fetch -q origin || exit 1
[ -z "$(git status --porcelain)" ] || { echo "primary worktree not clean"; exit 1; }
for wt in $(git worktree list --porcelain | awk '$1=="worktree"{print $2}'); do
  [ -z "$(git -C "$wt" status --porcelain)" ] || { echo "worktree not clean: $wt"; exit 1; }
done
[ "$(git rev-parse main)" = "$(git rev-parse origin/main)" ] || { echo "local main != origin/main; merge PR #13 first, then: git fetch origin && git branch -f main origin/main"; exit 1; }
gh api repos/jblossey/houserules --jq .id >/dev/null || { echo "gh not authenticated"; exit 1; }

echo "== 1. safety bundle of every ref"
BUNDLE="$HOME/houserules-pre-rewrite-$(date +%Y%m%d-%H%M%S).bundle"
git bundle create "$BUNDLE" --all || exit 1
echo "   $BUNDLE"

echo "== 2. rewrite every ref (branches, tags, remote-tracking refs)"
before=$(git rev-list --all --count)
: > "$MAP"
git filter-branch -f --tag-name-filter cat --commit-filter '
  [ "$GIT_AUTHOR_EMAIL" = "$OLD" ] && export GIT_AUTHOR_EMAIL="$NEW"
  [ "$GIT_COMMITTER_EMAIL" = "$OLD" ] && export GIT_COMMITTER_EMAIL="$NEW"
  new=$(git commit-tree "$@")
  echo "$GIT_COMMIT $new" >> "$MAP"
  echo "$new"
' -- --all || exit 1
git for-each-ref --format='%(refname)' refs/original/ | while read -r r; do git update-ref -d "$r"; done
after=$(git rev-list --all --count)
left=$(git log --all --format='%ae%n%ce' | grep -c "$OLD")
echo "   commits before/after: $before/$after; commits still carrying the old email: $left; map lines: $(wc -l < "$MAP")"
[ "$before" = "$after" ] && [ "$left" = 0 ] || { echo "verification failed; restore with: git clone $BUNDLE"; exit 1; }

echo "== 3. remap every pinned or cited sha in the tree (main)"
git checkout -q main || exit 1
while read -r old new; do
  [ "$old" = "$new" ] && continue
  git grep -lz --fixed-strings "$old" -- . ':!Cargo.lock' | xargs -0 -r sed -i "s/$old/$new/g"
  for len in 12 11 10 9 8 7; do
    op=${old:0:$len}; np=${new:0:$len}
    git grep -lzE "(^|[^0-9a-fA-F])$op([^0-9a-fA-F]|\$)" -- . ':!Cargo.lock' \
      | xargs -0 -r sed -i -E "s/(^|[^0-9a-fA-F])$op([^0-9a-fA-F]|\$)/\1$np\2/g"
  done
done < "$MAP"
echo "   files touched:"; git status --short | sed 's/^/     /'

echo "== 4. prove the pins resolve and the suite passes on the rewritten history"
cargo test --locked -q 2>&1 | grep -E 'test result|FAILED|^error' | sort | uniq -c | sed 's/^/   /'
cargo test --locked -q >/dev/null 2>&1 || { echo "tests failed on the remapped tree; stop here and inspect (nothing pushed)"; exit 1; }
mise run lint >/dev/null 2>&1 || { echo "lint failed on the remapped tree; stop here and inspect (nothing pushed)"; exit 1; }

echo "== 5. commit the map and this script as the record"
mkdir -p docs/history
cp "$MAP" "docs/history/$STAMP-author-email-rewrite-commit-map.txt"
cp "$0" "docs/history/$STAMP-author-email-rewrite.sh"
git add -A
git commit -q -F - <<EOF
chore(history): rewrite the commit author identity to the GitHub noreply address

HR-093, owner ruling $STAMP: every commit that carried the maintainer's personal email now
carries the GitHub noreply address, author and committer alike. Every sha changed. This commit
remaps each sha the tree pins or cites (gen-goldens' FROZEN_SHA, the parity fixtures' pinned
commits, backlog and knowledge prose) through the old-to-new map committed beside it under
docs/history/, together with the script that produced it. Test evidence: cargo test --locked
and mise run lint green on the remapped tree before the push.
EOF
git log --oneline -1 | sed 's/^/   /'

echo "== 6. push under a temporarily disabled ruleset"
gh api -X PUT "repos/jblossey/houserules/rulesets/$RULESET_MAIN" -f enforcement=disabled >/dev/null || exit 1
gh api -X PUT "repos/jblossey/houserules/rulesets/$RULESET_TAGS" -f enforcement=disabled >/dev/null || exit 1
ok=1
git push --force origin main || ok=0
git push --force origin --tags || ok=0
for b in $(git for-each-ref --format='%(refname:strip=3)' refs/remotes/origin | grep -vE '^(HEAD|main)$'); do
  git push --force origin "refs/remotes/origin/$b:refs/heads/$b" || ok=0
done
gh api -X PUT "repos/jblossey/houserules/rulesets/$RULESET_MAIN" -f enforcement=active >/dev/null
gh api -X PUT "repos/jblossey/houserules/rulesets/$RULESET_TAGS" -f enforcement=active >/dev/null
echo "   rulesets: $(gh api repos/jblossey/houserules/rulesets --jq '[.[]|"\(.name)=\(.enforcement)"]|join(" ")')"
[ "$ok" = 1 ] || { echo "a push failed; rulesets re-enabled; inspect before retrying"; exit 1; }

echo "== 7. verify the remote"
git fetch -q origin
echo "   origin/main == main: $([ "$(git rev-parse origin/main)" = "$(git rev-parse main)" ] && echo yes || echo NO)"
echo "   old email on origin/main: $(git log origin/main --format='%ae%n%ce' | grep -c "$OLD")"
git checkout -q batch-21
echo "done. Follow-ups: comment '@dependabot rebase' on PR #11; the release-please PR refreshes on the next push to main."
