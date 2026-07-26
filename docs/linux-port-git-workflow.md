# Linux port Git workflow

This checkout contains two separate repositories:

- `cmux/` — the cmux fork and Linux port; run cmux Git commands here.
- `../ghostty/` — a separate Ghostty checkout; do not treat the parent directory as a repository.

## Remotes and permanent branches

- `upstream` is `manaflow-ai/cmux`, the source repository.
- `origin` is `siure/cmux`, the working fork.
- `main` mirrors `upstream/main`; do not develop directly on it.
- `feat/linux-port` is the canonical Linux integration branch.
- `archive/linux-port-pre-rebase` preserves old unmatched checkpoint commits. It is reference-only and must not receive new work.

## Agent workflow

1. Start clean and refresh references:

   ```bash
   git fetch --all --prune
   git status
   ```

2. For Linux work, branch from the integration branch:

   ```bash
   git switch feat/linux-port
   git pull --ff-only
   git switch -c fix/linux-<short-name>
   ```

3. Keep commits focused, push the topic branch, and open the PR against the fork's integration branch:

   ```bash
   git push -u origin HEAD
   gh pr create --repo siure/cmux --base feat/linux-port
   ```

4. Delete topic branches after merge. Use names such as `fix/linux-*`, `feat/linux-*`, or `chore/linux-*`; do not create tool-named branches such as `codex/*`.

5. Update the fork mirror only by fast-forwarding from upstream:

   ```bash
   git switch main
   git fetch upstream
   git merge --ff-only upstream/main
   git push origin main
   ```

6. Bring upstream changes into `feat/linux-port` with an explicit PR or merge. Do not force-push or rewrite the shared integration branch unless the user explicitly approves it.

For work intended directly for upstream, create a separate topic branch from `upstream/main`; do not base it on the full Linux integration branch.
