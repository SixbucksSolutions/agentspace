#!/bin/bash
# Installs the agent toolchain for the ubuntu user. Safe to run again.
# Run from the laptop:  ssh agentspace 'bash -s' < box/bootstrap.sh
# From v0.07 the operator's `agentspace-operator bootstrap` runs it (D46).
set -euo pipefail

echo "== build-essential (gcc, for linking)"
if ! dpkg -s build-essential >/dev/null 2>&1; then
  sudo apt-get update -qq
  sudo apt-get install -y -qq build-essential
fi

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

echo "== Rust (rustup for this user; stable, rustfmt, clippy)"
if [ ! -x "$HOME/.cargo/bin/rustup" ]; then
  # The installer adds ". $HOME/.cargo/env" to the shell profile itself.
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable \
      --component rustfmt --component clippy
fi
export PATH="$HOME/.cargo/bin:$PATH"
# On every run, so a host whose Rust was installed by hand ends with this one.
rustup toolchain install stable --profile minimal --component rustfmt --component clippy
rustup default stable
# Links in ~/.local/bin, which a non-interactive ssh shell has on PATH (Ubuntu's ~/.bashrc returns
# early there, so rustup's profile edit is never read). Each points at rustup's proxy, so the
# toolchain is the one `rustup default` names. This makes the box's hand-made links the
# bootstrap's: an existing file or link of the same name is replaced.
mkdir -p "$HOME/.local/bin"
for tool in cargo rustc rustup rustfmt cargo-fmt cargo-clippy clippy-driver; do
  # Only a regular file or a link is replaced; a directory of that name is left alone.
  if [ -d "$HOME/.local/bin/$tool" ] && [ ! -L "$HOME/.local/bin/$tool" ]; then
    continue
  fi
  ln -sfn "$HOME/.cargo/bin/$tool" "$HOME/.local/bin/$tool"
done

echo "== Clean caches"
npm cache clean --force >/dev/null 2>&1 || true
sudo apt-get clean

echo "== Versions"
# ~/.local/bin is first, so the Rust links above resolve to rustup's proxies and print its default.
export PATH="$HOME/.local/bin:$PATH"
printf 'Node    %s\n' "$(node --version)"
printf 'Claude  %s\n' "$(claude --version)"
printf 'Codex   %s\n' "$(codex --version)"
printf 'uv      %s\n' "$(uv --version)"
printf 'gh      %s\n' "$(gh --version | head -1)"
printf 'gcc     %s\n' "$(gcc --version | head -1)"
printf 'rustc   %s\n' "$(rustc --version)"
printf 'cargo   %s\n' "$(cargo --version)"
printf 'rustfmt %s\n' "$(rustfmt --version)"
printf 'clippy  %s\n' "$(cargo clippy --version)"
echo "== Disk"
df -h /
