#!/bin/bash
# Installs the agentspace system files from the directory this script is in.
# Run as root; agentspace-operator sync copies the files up and runs it. Safe to run again: every
# sync runs it, on every host.
set -euo pipefail
cd "$(dirname "$0")"

# The temporary files this run makes, removed however it exits.
backup=""
env_tmp=""
cleanup() {
  if [ -n "$backup" ]; then rm -rf "$backup"; fi
  if [ -n "$env_tmp" ]; then rm -f "$env_tmp"; fi
}
trap cleanup EXIT

# sync sends the CA's public key beside these files; it is never in the repo (decision D17).
if [ ! -s agentspace_ca.pub ]; then
  echo "install.sh: agentspace_ca.pub is missing; run agentspace-operator sync, which sends it" >&2
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
install -D -m 0755 agentspace-agents          /usr/local/bin/agentspace-agents
install -D -m 0755 agentspace-sample          /usr/local/bin/agentspace-sample
install -D -m 0644 agentspace-idle.service    /etc/systemd/system/agentspace-idle.service
install -D -m 0644 agentspace-idle.timer      /etc/systemd/system/agentspace-idle.timer
install -D -m 0644 agentspace-patch.service   /etc/systemd/system/agentspace-patch.service
systemctl daemon-reload
systemctl enable --now agentspace-idle.timer
systemctl enable agentspace-patch.service
echo "Installed: idle check (timer $(systemctl is-active agentspace-idle.timer)), patch at boot ($(systemctl is-enabled agentspace-patch.service))"

# Put the login user's tool folders on the PATH of every session, a non-interactive ssh command
# included: it reads /etc/environment through PAM, while Ubuntu's ~/.bashrc returns early for it,
# before any PATH change there (D61, #0166).
# PAM does not expand variables there, so the folders are absolute. The PATH line's existing value
# is kept, and each folder is appended only if it is absent, in this order; only a file with no
# PATH line gets one, Ubuntu's default followed by the folders (architect, 2026-11-21, for
# v0.08-T01). Every other line is left as it was, and the file is replaced by a rename, so a
# reader never sees half of it. A second run finds nothing to add and leaves it untouched.
env_file=/etc/environment
path_default=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/usr/games:/usr/local/games:/snap/bin
path_user=(/home/ubuntu/.cargo/bin /home/ubuntu/.local/bin /home/ubuntu/.local/share/fnm/aliases/default/bin)

# Prints a PATH line of /etc/environment with each of path_user's folders appended when absent.
# pam_env ends a value at its first "#", so the value is what comes before that and before the
# blanks ahead of it; the trailing text is put back after the value, its closing quote included.
# The line's prefix ("export " as pam_env allows), the value's quotes and a CR ending are kept as
# they were. Returns 1, printing nothing, for a line that does not set PATH, and 2, printing
# nothing, for one whose value is neither plain (no quote character in it) nor quoted whole (one
# matching pair of quotes around it and no other quote character).
path_tail_re='^([^#]*[^#[:space:]])?([[:space:]]*(#.*)?)$'
with_user_path() {
  local line=$1 cr="" prefix value tail q="" dir
  if [[ $line == *$'\r' ]]; then cr=$'\r'; line=${line%$'\r'}; fi
  [[ $line =~ ^([[:space:]]*(export[[:space:]]+)?PATH=)(.*)$ ]] || return 1
  prefix=${BASH_REMATCH[1]}
  value=${BASH_REMATCH[3]}
  [[ $value =~ $path_tail_re ]] || return 2
  value=${BASH_REMATCH[1]}
  tail=${BASH_REMATCH[2]}
  case $value in
    *[\"\']*)
      q=${value:0:1}
      case $q in \"|\') ;; *) return 2 ;; esac
      if [ "${#value}" -lt 2 ] || [ "${value: -1}" != "$q" ]; then return 2; fi
      value=${value:1:${#value}-2}
      case $value in *[\"\']*) return 2 ;; esac
      ;;
  esac
  for dir in "${path_user[@]}"; do
    case ":$value:" in
      *":$dir:"*) ;;
      *) value=${value:+$value:}$dir ;;
    esac
  done
  printf '%s%s%s%s%s%s' "$prefix" "$q" "$value" "$q" "$tail" "$cr"
}

env_tmp=$(mktemp /etc/.environment.XXXXXX)
path_lines=0
line_no=0
last_nl=$'\n'
if [ -f "$env_file" ]; then
  while true; do
    if IFS= read -r line; then last_nl=$'\n'; elif [ -n "$line" ]; then last_nl=""; else break; fi
    line_no=$((line_no + 1))
    path_rc=0
    new=$(with_user_path "$line") || path_rc=$?
    if [ "$path_rc" -eq 0 ]; then
      line=$new
      path_lines=$((path_lines + 1))
    elif [ "$path_rc" -eq 2 ]; then
      echo "install.sh: line $line_no of $env_file sets PATH to a value that is neither plain nor quoted whole; the file is left as it was, so fix that line by hand and run agentspace-operator sync again" >&2
      exit 1
    fi
    printf '%s%s' "$line" "$last_nl"
    if [ -z "$last_nl" ]; then break; fi
  done < "$env_file" > "$env_tmp"
fi
if [ "$path_lines" -eq 0 ]; then
  # A last line with no line feed gets one, so the new line is a line of its own.
  if [ -z "$last_nl" ]; then printf '\n' >> "$env_tmp"; fi
  with_user_path "PATH=\"$path_default\"" >> "$env_tmp"
  printf '\n' >> "$env_tmp"
fi
if [ -f "$env_file" ] && cmp -s "$env_tmp" "$env_file"; then
  rm -f "$env_tmp"
  env_state="already there"
else
  chmod 0644 "$env_tmp"
  chown root:root "$env_tmp"
  mv -f "$env_tmp" "$env_file"
  env_state="added"
fi
env_tmp=""
# Check it as the sshd settings are checked below: every PATH line now holds every folder in the
# value pam_env reads, parsed as above, and none is neither plain nor quoted whole.
path_lines=0
while IFS= read -r line || [ -n "$line" ]; do
  path_rc=0
  new=$(with_user_path "$line") || path_rc=$?
  if [ "$path_rc" -eq 0 ]; then
    path_lines=$((path_lines + 1))
    if [ "$new" != "$line" ]; then path_lines=-1; break; fi
  elif [ "$path_rc" -eq 2 ]; then
    path_lines=-1
    break
  fi
done < "$env_file"
if [ "$path_lines" -lt 1 ]; then
  echo "install.sh: $env_file has no PATH line holding ${path_user[*]} after the install wrote it" >&2
  exit 1
fi
echo "Installed: $env_file's PATH holds the ubuntu user's tool folders ($env_state)"

# Trust the agentspace SSH CA and close root login (decision D7), and keep refused logins from
# locking everyone out (E3.01) and a dead client's session from holding the board's forwards
# (0.02.T1.1): one transaction for the CA key and both drop-ins. The new files stay only if
# sshd -t accepts them; otherwise every file this run changed is put back, so a later restart or
# reboot never meets a bad config. Other drop-ins in sshd_config.d are left alone.
ca_pub=/etc/ssh/agentspace_ca.pub
ca_dropin=/etc/ssh/sshd_config.d/50-agentspace-ca.conf
logins_dropin=/etc/ssh/sshd_config.d/51-agentspace-logins.conf
backup=$(mktemp -d)
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

# Start this boot's patch run if it has not finished (D61's change, #0163): a host whose first
# boot came before sync installed the patch service has had no patch run, and sync waits for the
# marker after this install. It is started last, so its package upgrades and any reboot never
# overlap the SSH steps above, and with --no-block, so this install returns at once. With the
# marker present it is not started. A patch run already under way removed the marker as it
# began; its start job is still queued, so systemd merges this start into that job and no
# second run begins. A run that ended without the marker (a failure) is started again.
if [ -e /run/agentspace/patched ]; then
  echo "Patch run: finished this boot; not started again"
else
  patch_before=$(systemctl is-active agentspace-patch.service || true)
  if ! systemctl start --no-block agentspace-patch.service; then
    echo "install.sh: systemd refused to start agentspace-patch.service; this boot's patch run has not started" >&2
    exit 1
  fi
  if [ "$patch_before" = activating ]; then
    echo "Patch run: already running this boot; not started again"
  else
    echo "Patch run: started for this boot, as /run/agentspace/patched is missing"
  fi
fi
