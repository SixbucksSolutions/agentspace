#!/bin/bash
# Installs the agentspace system files from the directory this script is in.
# Run as root; scripts\agentspace.ps1 sync copies the files up and runs it.
set -euo pipefail
cd "$(dirname "$0")"

# The project was first named agentbox. Retire that name's units and files, and carry this
# boot's patch marker and the last patch reboot time over, so a box that already patched this
# boot is not seen as patching again.
if [ -e /etc/systemd/system/agentbox-idle.timer ] || [ -e /etc/systemd/system/agentbox-patch.service ]; then
  systemctl disable --now agentbox-idle.timer 2>/dev/null || true
  systemctl disable agentbox-patch.service 2>/dev/null || true
  if [ -f /run/agentbox/patched ]; then
    install -D -m 0644 /dev/null /run/agentspace/patched
  fi
  if [ -f /var/lib/agentbox/last-patch-reboot ]; then
    install -D -m 0644 /var/lib/agentbox/last-patch-reboot /var/lib/agentspace/last-patch-reboot
  fi
  rm -f /etc/systemd/system/agentbox-idle.service /etc/systemd/system/agentbox-idle.timer \
    /etc/systemd/system/agentbox-patch.service /usr/local/sbin/agentbox-idle-check \
    /usr/local/sbin/agentbox-patch /usr/local/bin/agentbox-tools
  rm -rf /etc/agentbox /run/agentbox /var/lib/agentbox
  echo "Retired the agentbox units and files"
fi

install -D -m 0644 idle.conf                  /etc/agentspace/idle.conf
install -D -m 0755 agentspace-idle-check      /usr/local/sbin/agentspace-idle-check
install -D -m 0755 agentspace-patch           /usr/local/sbin/agentspace-patch
install -D -m 0755 agentspace-tools           /usr/local/bin/agentspace-tools
install -D -m 0755 localswim                  /usr/local/bin/localswim
install -D -m 0644 agentspace-idle.service    /etc/systemd/system/agentspace-idle.service
install -D -m 0644 agentspace-idle.timer      /etc/systemd/system/agentspace-idle.timer
install -D -m 0644 agentspace-patch.service   /etc/systemd/system/agentspace-patch.service
systemctl daemon-reload
systemctl enable --now agentspace-idle.timer
systemctl enable agentspace-patch.service
echo "Installed: idle check (timer $(systemctl is-active agentspace-idle.timer)), patch at boot ($(systemctl is-enabled agentspace-patch.service))"
