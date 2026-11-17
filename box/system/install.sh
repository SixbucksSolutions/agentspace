#!/bin/bash
# Installs the agentbox system files from the directory this script is in.
# Run as root; scripts\agentbox.ps1 sync copies the files up and runs it.
set -euo pipefail
cd "$(dirname "$0")"
install -D -m 0644 idle.conf                /etc/agentbox/idle.conf
install -D -m 0755 agentbox-idle-check      /usr/local/sbin/agentbox-idle-check
install -D -m 0755 agentbox-patch           /usr/local/sbin/agentbox-patch
install -D -m 0755 agentbox-tools           /usr/local/bin/agentbox-tools
install -D -m 0755 localswim                /usr/local/bin/localswim
install -D -m 0644 agentbox-idle.service    /etc/systemd/system/agentbox-idle.service
install -D -m 0644 agentbox-idle.timer      /etc/systemd/system/agentbox-idle.timer
install -D -m 0644 agentbox-patch.service   /etc/systemd/system/agentbox-patch.service
systemctl daemon-reload
systemctl enable --now agentbox-idle.timer
systemctl enable agentbox-patch.service
echo "Installed: idle check (timer $(systemctl is-active agentbox-idle.timer)), patch at boot ($(systemctl is-enabled agentbox-patch.service))"
