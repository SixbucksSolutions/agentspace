#!/bin/bash
# Installs the agentspace system files from the directory this script is in.
# Run as root; scripts\agentspace.ps1 sync copies the files up and runs it.
set -euo pipefail
cd "$(dirname "$0")"

# sync sends the CA's public key beside these files; it is never in the repo (decision D17).
if [ ! -s agentspace_ca.pub ]; then
  echo "install.sh: agentspace_ca.pub is missing; run scripts\\agentspace.ps1 sync, which sends it" >&2
  exit 1
fi

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
install -D -m 0755 agentspace-footprint       /usr/local/bin/agentspace-footprint
install -D -m 0755 agentspace-sample          /usr/local/bin/agentspace-sample
install -D -m 0644 agentspace-idle.service    /etc/systemd/system/agentspace-idle.service
install -D -m 0644 agentspace-idle.timer      /etc/systemd/system/agentspace-idle.timer
install -D -m 0644 agentspace-patch.service   /etc/systemd/system/agentspace-patch.service
systemctl daemon-reload
systemctl enable --now agentspace-idle.timer
systemctl enable agentspace-patch.service
echo "Installed: idle check (timer $(systemctl is-active agentspace-idle.timer)), patch at boot ($(systemctl is-enabled agentspace-patch.service))"

# Trust the agentspace SSH CA and close root login (decision D7), and keep refused logins from
# locking everyone out (E3.01) and a dead client's session from holding the board's forwards
# (0.02.T1.1): one transaction for the CA key and both drop-ins. The new files stay only if
# sshd -t accepts them; otherwise every file this run changed is put back, so a later restart or
# reboot never meets a bad config. Other drop-ins in sshd_config.d are left alone.
ca_pub=/etc/ssh/agentspace_ca.pub
ca_dropin=/etc/ssh/sshd_config.d/50-agentspace-ca.conf
logins_dropin=/etc/ssh/sshd_config.d/51-agentspace-logins.conf
backup=$(mktemp -d)
trap 'rm -rf "$backup"' EXIT
for f in "$ca_pub" "$ca_dropin" "$logins_dropin"; do
  if [ -e "$f" ]; then cp -p "$f" "$backup/"; fi
done
install -m 0644 agentspace_ca.pub          "$ca_pub"
install -m 0644 50-agentspace-ca.conf      "$ca_dropin"
install -m 0644 51-agentspace-logins.conf  "$logins_dropin"
if ! sshd -t; then
  for f in "$ca_pub" "$ca_dropin" "$logins_dropin"; do
    if [ -e "$backup/$(basename "$f")" ]; then cp -p "$backup/$(basename "$f")" "$f"; else rm -f "$f"; fi
  done
  echo "install.sh: sshd -t rejected the new SSH config; put the old files back and did not reload ssh.service" >&2
  exit 1
fi
systemctl reload ssh.service
effective=$(sshd -T | grep -Ei '^(trustedusercakeys|permitrootlogin) ' | sort | tr '\n' ' ')
if [ "$effective" != "permitrootlogin no trustedusercakeys $ca_pub " ]; then
  echo "install.sh: sshd reports '$effective'; another setting overrides 50-agentspace-ca.conf" >&2
  exit 1
fi
effective=$(sshd -T | grep -Ei '^(clientalivecountmax|clientaliveinterval|logingracetime|persourcepenaltyexemptlist) ' | sort | tr '\n' ' ')
if [ "$effective" != "clientalivecountmax 5 clientaliveinterval 15 logingracetime 30 persourcepenaltyexemptlist 127.0.0.1 " ]; then
  echo "install.sh: sshd reports '$effective'; another setting overrides 51-agentspace-logins.conf" >&2
  exit 1
fi
echo "Installed: SSH trusts the agentspace CA ($(ssh-keygen -l -f "$ca_pub" | cut -d' ' -f2)), root login closed"
echo "Installed: 127.0.0.1 exempt from sshd penalties, login grace time 30 seconds"
echo "Installed: a session whose client has gone is dropped about 90 seconds after it goes quiet (probe every 15 seconds, 5 unanswered)"
