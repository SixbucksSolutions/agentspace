#!/bin/bash
# Installs the agent toolchain for the ubuntu user. Safe to run again.
# Run from the laptop:  ssh agentspace 'bash -s' < box/bootstrap.sh
set -euo pipefail

echo "== GitHub CLI"
if ! command -v gh >/dev/null; then
  sudo install -d -m 0755 /etc/apt/keyrings
  curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
    | sudo tee /etc/apt/keyrings/githubcli-archive-keyring.gpg >/dev/null
  echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
    | sudo tee /etc/apt/sources.list.d/github-cli.list >/dev/null
  sudo apt-get update -qq
  sudo apt-get install -y -qq gh
fi

echo "== Node (fnm; each project's .node-version picks its version)"
if [ ! -x "$HOME/.local/share/fnm/fnm" ]; then
  curl -fsSL https://fnm.vercel.app/install | bash -s -- --skip-shell
fi
export PATH="$HOME/.local/share/fnm:$PATH"
if ! grep -q 'fnm env' "$HOME/.bashrc"; then
  cat >> "$HOME/.bashrc" <<'EOF'

# fnm: switch Node to the project's .node-version on cd.
export PATH="$HOME/.local/share/fnm:$PATH"
eval "$(fnm env --use-on-cd --version-file-strategy=recursive --shell bash)"
EOF
fi
eval "$(fnm env --shell bash)"
# The newest Node LTS; agentspace-tools moves the default forward at every boot.
fnm install --lts
fnm default "$(fnm ls-remote --lts | awk '{print $1}' | tail -1)"

echo "== Claude Code (native installer; updates itself)"
command -v claude >/dev/null || curl -fsSL https://claude.ai/install.sh | bash

echo "== Codex"
npm install -g @openai/codex@latest --no-fund --no-audit --loglevel=error

echo "== uv"
command -v uv >/dev/null || curl -LsSf https://astral.sh/uv/install.sh | sh

echo "== Clean caches"
npm cache clean --force >/dev/null 2>&1 || true
sudo apt-get clean

echo "== Versions"
export PATH="$HOME/.local/bin:$PATH"
printf 'Node    %s\n' "$(node --version)"
printf 'Claude  %s\n' "$(claude --version)"
printf 'Codex   %s\n' "$(codex --version)"
printf 'uv      %s\n' "$(uv --version)"
printf 'gh      %s\n' "$(gh --version | head -1)"
echo "== Disk"
df -h /
