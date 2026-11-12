#!/usr/bin/env bash
set -euo pipefail

BUNDLE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_DIR=/etc/led-lantern
ENV_FILE=$CONFIG_DIR/led-lantern.env
MEDIAMTX_VERSION=v1.6.0
MEDIAMTX_CONFIG=/usr/local/etc/mediamtx.yml
BOOT_CMDLINE=/boot/firmware/cmdline.txt
BOOT_CONFIG=/boot/firmware/config.txt
SOUND_BLACKLIST=/etc/modprobe.d/blacklist-snd_bcm2835.conf
is_reboot_needed=false

step() {
  echo
  echo "==> $*"
}

step "ffmpeg"
if ! command -v ffmpeg >/dev/null; then
  sudo apt-get update
  sudo apt-get install -y ffmpeg
fi
ffmpeg -version | head -1

step "led-lantern"
sudo install -m 755 "$BUNDLE_DIR/led-lantern" /usr/local/bin/led-lantern
echo "Installed $(/usr/local/bin/led-lantern --version)"

step "Configuration"
sudo install -d -m 700 "$CONFIG_DIR"
sudo install -d -m 755 /var/cache/led-lantern
if ! sudo test -f "$ENV_FILE"; then
  sudo install -m 600 "$BUNDLE_DIR/deploy/led-lantern.env.example" "$ENV_FILE"
  echo "Created $ENV_FILE. Fill in the Slack channel and tokens, then run:"
  echo "  sudo systemctl restart led-lantern"
fi

step "MediaMTX"
if [ ! -x /usr/local/bin/mediamtx ]; then
  download_dir=$(mktemp -d)
  curl -sSfL "https://github.com/bluenviron/mediamtx/releases/download/${MEDIAMTX_VERSION}/mediamtx_${MEDIAMTX_VERSION}_linux_arm64v8.tar.gz" |
    tar xz -C "$download_dir"
  sudo install -m 755 "$download_dir/mediamtx" /usr/local/bin/mediamtx
  sudo install -d /usr/local/etc
  if [ ! -f "$MEDIAMTX_CONFIG" ]; then
    sudo install -m 644 "$download_dir/mediamtx.yml" "$MEDIAMTX_CONFIG"
  fi
  rm -rf "$download_dir"
fi
sudo sed -i -E 's/^api: no$/api: yes/; s/^apiAddress: .*/apiAddress: 127.0.0.1:9997/' "$MEDIAMTX_CONFIG"
sudo install -m 644 "$BUNDLE_DIR/deploy/mediamtx.service" /etc/systemd/system/mediamtx.service

step "LED timing"
if ! grep -q 'isolcpus=3' "$BOOT_CMDLINE"; then
  sudo sed -i 's/$/ isolcpus=3/' "$BOOT_CMDLINE"
  is_reboot_needed=true
fi
if [ ! -f "$SOUND_BLACKLIST" ]; then
  echo "blacklist snd_bcm2835" | sudo tee "$SOUND_BLACKLIST" >/dev/null
  is_reboot_needed=true
fi

step "SD card writes"
sudo install -d /etc/systemd/journald.conf.d
sudo install -m 644 "$BUNDLE_DIR/deploy/journald.conf" /etc/systemd/journald.conf.d/led-lantern.conf
sudo systemctl restart systemd-journald
echo "System log kept in RAM only"
if systemctl is-enabled --quiet dphys-swapfile.service 2>/dev/null; then
  sudo dphys-swapfile swapoff
  sudo dphys-swapfile uninstall
  sudo systemctl disable dphys-swapfile.service
  echo "Swap file removed"
fi
if [ ! -f /etc/systemd/system/tmp.mount ]; then
  sudo cp /usr/share/systemd/tmp.mount /etc/systemd/system/tmp.mount
  sudo systemctl enable tmp.mount
  echo "/tmp moves to RAM after a reboot"
  is_reboot_needed=true
fi
for timer in apt-daily.timer apt-daily-upgrade.timer man-db.timer dpkg-db-backup.timer; do
  if systemctl is-enabled --quiet "$timer" 2>/dev/null; then
    sudo systemctl disable --now "$timer"
    echo "Disabled $timer"
  fi
done
if [ -x /etc/cron.hourly/fake-hwclock ]; then
  sudo chmod -x /etc/cron.hourly/fake-hwclock
  echo "Disabled the hourly clock save"
fi
if ! awk '$2 == "/" && $4 ~ /noatime/ { found = 1 } END { exit !found }' /etc/fstab; then
  echo "WARNING: / is not mounted with noatime in /etc/fstab, so every file read is also a write."
fi

step "Standby power"
for service in bluetooth.service hciuart.service ModemManager.service; do
  if systemctl is-enabled --quiet "$service" 2>/dev/null; then
    sudo systemctl disable --now "$service"
    echo "Disabled $service"
  fi
done
if ! grep -q '^dtoverlay=disable-bt' "$BOOT_CONFIG"; then
  printf '\n[all]\ndtoverlay=disable-bt\n' | sudo tee -a "$BOOT_CONFIG" >/dev/null
  echo "Bluetooth radio switches off after a reboot"
  is_reboot_needed=true
fi

step "Retiring the PM2 scripts"
if systemctl is-enabled --quiet pm2-root.service 2>/dev/null; then
  sudo systemctl disable --now pm2-root.service
  echo "Stopped and disabled pm2-root.service"
else
  echo "Nothing to do"
fi

step "Services"
sudo install -m 644 "$BUNDLE_DIR/deploy/led-lantern.service" /etc/systemd/system/led-lantern.service
sudo systemctl daemon-reload
sudo systemctl enable mediamtx.service led-lantern.service
sudo systemctl restart mediamtx.service led-lantern.service
systemctl --no-pager --lines=0 status led-lantern.service

if $is_reboot_needed; then
  echo
  echo "Boot settings changed. Reboot to apply them: sudo reboot"
fi
