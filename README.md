# led-lantern

Rust service that drives the LED Lantern. It shows whatever it received last:
a Slack message, a YouTube video posted in Slack, or a video stream published
to the lantern.

## Hardware

- 6 flexible 64x32 RGB LED tiles chained into one 384x32 px surface around the lantern.
- Raspberry Pi 4 Model B with an Adafruit RGB Matrix Bonnet (`adafruit-hat-pwm` GPIO mapping).
- Debian 12 (bookworm), 64-bit.

The panel is driven through hzeller's
[rpi-rgb-led-matrix](https://github.com/hzeller/rpi-rgb-led-matrix) C++ library via the
`rpi-led-matrix` crate, which compiles the library from source. That build uses the Pi's
own `make` and `g++`, so the app is built on the lantern rather than cross-compiled.

## What it shows

### Slack

One channel, plus direct messages to the bot.

- Every new message scrolls across at least once, oldest first, channel and DMs mixed.
  While nothing new is waiting, the latest message keeps looping.
- A first pass of a text message is never cut short. A repeat is cut short as soon as
  something new arrives.
- A deleted message comes off the panel straight away. An edited message shows its new
  text from its next pass.
- At startup the channel's latest message is shown. Older messages are kept as fallbacks
  for when the current one is deleted. Messages sent while the service was down are not
  replayed.
- A message starting `ANNOUNCEMENT:` (any case, Slack bold/italic allowed) cuts in
  immediately: a 5 second white `ANNOUNCEMENT •` intro, then the message looping as a
  ticker, with the intro returning every 5 minutes. It holds the panel until it is deleted
  or a newer announcement replaces it. Messages arriving meanwhile wait their turn.
- Emoji shortcodes become ASCII the font can draw (`:wave:` becomes `o/`). Mentions,
  channel links and URLs become plain text.

New messages arrive over Slack Socket Mode, a WebSocket connection Slack pushes events
down. That needs the app-level token (`xapp-…`). The channel is also polled every 5
seconds, which picks up edits and deletions. Without the app token only polling runs,
and DMs are not seen.

### YouTube

A Slack message containing a YouTube link plays that video, looping, until something
newer arrives. A new message cuts a video immediately. The video is downloaded at 240p
or below with [yt-dlp](https://github.com/yt-dlp/yt-dlp). If the download fails, the
message text is shown instead.

The service keeps its own yt-dlp binary in `/var/lib/led-lantern/yt-dlp`. It downloads
it on first start, then runs `yt-dlp --update` on every start and every 24 hours after.

### Video streams

Anything published to the lantern's [MediaMTX](https://github.com/bluenviron/mediamtx)
streaming server plays as soon as the stream is live, and stops when it ends. For example:

```
ffmpeg -re -stream_loop -1 -i input.mov -c:v libx264 -preset ultrafast \
  -f rtsp -rtsp_transport tcp rtsp://led-lantern.local:8554/my-stream
```

The service polls the MediaMTX API on `127.0.0.1:9997` once a second to see which streams
are live. When a stream ends, the panel falls back to the latest Slack message.

### Home Assistant switch and standby

The lantern announces itself to Home Assistant over MQTT discovery as
`switch.led_lantern_display`, using the same topics as the old Node script, so existing
automations keep working. The broker is set with `MQTT_HOST`. Without it there is no switch
and the display stays on.

Switching it off is the overnight standby, and goes as low as software can:

- The LED driver is shut down completely, not just set to black. The hzeller library's
  refresh thread normally keeps CPU core 3 at 100% even when the panel is black; in standby
  that thread is stopped, the LEDs are cleared, and the core idles.
- Any video's ffmpeg process is stopped.
- Slack keeps being watched, so messages that arrive overnight play once the display is
  switched back on.

The state is saved to `/var/lib/led-lantern/power-state`, so a reboot overnight stays off.

The LED tiles and their 5V supply still draw standby current while the Pi is in standby.
Only a relay or smart plug on the supply can remove that.

### Scaling

Video that doesn't match the 384x32 surface is scaled down to fit, keeping its aspect
ratio, then repeated across the rest of the surface. A 16:9 video becomes a 57x32 tile
repeated around the lantern. The tile count doesn't divide the circumference exactly, so
there is a seam where the last partial tile meets the first.

## Setting up a lantern

From a Mac with SSH access to the lantern:

```
deploy/deploy.sh random@led-lantern.local
```

This copies the repo to `~/led-lantern` on the lantern and runs
`deploy/setup-lantern.sh` there. The setup script can also be run directly on a lantern
after cloning the repo. It is safe to re-run. It:

1. Installs ffmpeg with apt.
2. Installs Rust with rustup if it's missing, then builds and installs
   `/usr/local/bin/led-lantern`.
3. Creates `/etc/led-lantern/led-lantern.env` from `deploy/led-lantern.env.example` if
   it doesn't exist.
4. Installs MediaMTX `v1.6.0` if it's missing, and turns on its API, bound to localhost only.
5. Sets the LED timing settings from the hzeller library's docs: `isolcpus=3` in
   `/boot/firmware/cmdline.txt` reserves CPU core 3 for refreshing the panel, and the
   onboard sound driver `snd_bcm2835` is blacklisted because it uses the PWM hardware the
   library needs for timing the LEDs. Either change needs a reboot, and the script says so.
6. Cuts SD card writes, see below.
7. Turns off Bluetooth and ModemManager, which the lantern doesn't use, to lower standby power.
8. Stops and disables `pm2-root.service`, which ran the old Node and Python scripts that
   drove the same panel.
9. Installs and starts the `led-lantern` and `mediamtx` systemd services. Both start on
   boot and restart if they exit.

### SD card writes

The lantern runs from an SD card, so the setup script removes the regular writes:

- The system log (journald) is kept in RAM only, capped at 32 MB. Logs don't survive a reboot.
  Old logs already in `/var/log/journal` are left in place and can be deleted.
- The swap file is removed. The Pi has 4 GB and the app uses far less.
- `/tmp` is mounted in RAM (from the next reboot).
- The daily apt package-list refresh, man-db index rebuild and dpkg database backup are
  disabled. Run `sudo apt update` by hand before installing anything.
- The hourly save of the clock by `fake-hwclock` is disabled. The clock is still saved at
  shutdown and set over the network at boot.
- The script warns if `/` isn't mounted with `noatime`, which stops file reads from being
  recorded as writes.

The app itself only writes to the card when the power state changes or yt-dlp updates.
Downloaded YouTube videos and yt-dlp's cache go to `/run/led-lantern`, which is in RAM.
Building the app during setup does write to the card, but that only happens on deploy.

### Configuration

`/etc/led-lantern/led-lantern.env` (root only, mode 600):

| Variable | Meaning |
| --- | --- |
| `SLACK_CHANNEL` | ID of the channel to show, e.g. `C0CGBT4ETFZ` |
| `SLACK_BOT_TOKEN` | Bot token, `xoxb-…`. Reads history and user names. |
| `SLACK_APP_TOKEN` | App-level token, `xapp-…`. Enables Socket Mode. Optional. |
| `MQTT_HOST` | MQTT broker for the Home Assistant switch, e.g. `10.112.10.10`. Optional. |
| `MQTT_PORT` | Defaults to `1883` |
| `MQTT_USERNAME` | Defaults to `led-lantern` |
| `MQTT_PASSWORD` | Broker password |
| `RUST_LOG` | Log level, e.g. `info` or `debug` |

After editing it: `sudo systemctl restart led-lantern`.

### Logs

```
journalctl -u led-lantern -f
```

## Development

On a Mac, the `rpi-led-matrix` crate is built with its `c-stubs` feature, which replaces
the C++ library with empty functions. The app compiles and the tests run, but nothing
reaches a panel.

```
cargo test
cargo clippy --all-targets -- -D warnings
```
