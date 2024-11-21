# Squirtinator

🚧 This repo is under construction. 🚧

The Squirtinator is a DIY sex toy that uses a diaphragm pump to deliver lube
through a tube at high velocity.

It can be remotely controlled over WiFi via a self-hosted web interface
designed for mobile. It doesn't need an internet connection, a local network,
or any apps to function. You can control it either via its own WiFi hotspot or
by connecting it to your local network.

## Usage

1. Connect the toy to power.
2. Connect to the toy's WiFi hotspot, called `Squirtinator` by default.
3. Open your browser and go to <http://192.168.0.1>. You may need to turn off
   mobile data if you're on a cellular device.

From here, you can control the toy in your browser by staying connected to its
WiFi hotspot. However, you can also connect the toy to your local network so
you can stay on your own WiFi and don't have to switch networks. To do this:

1. Open the setting page in your browser.
2. Enter your WiFi name (SSID) and password.
3. Click "Save" and restart the toy (unplug it and plug it back in).

When it powers back on, you should be able to access it at
<http://squirtinator.local>.

## Hardware

This project uses the open hardware [Rust ESP development
board](https://github.com/esp-rs/esp-rust-board), based on the ESP32-C3
microcontroller.

## Prerequisites

This repo builds the firmware in a container. To build the firmware, you'll
need to have [podman](https://podman.io/docs/installation) installed and
running in rootless mode.

To flash the firmware, you'll need to install:

- [just](https://github.com/casey/just?tab=readme-ov-file#installation)
- [espflash](https://github.com/esp-rs/espflash/tree/main/espflash#installation)

If you're on Linux, make sure your user has the necessary permissions to read
and write the serial port over USB. See
[here](https://docs.espressif.com/projects/esp-idf/en/latest/esp32/get-started/establish-serial-connection.html#linux-dialout-group)
for instructions.

## Flashing

Look at the [config.toml](./config.toml) file to see the available build-time
config options. The default values should work for most cases.

To build and flash the firmware with release optimizations, run:

```sh
just flash
```

## Development

To flash the firmware and watch the logs:

```sh
just dev
```

You can run any Cargo command like this:

```sh
just cargo check
```

## Troubleshooting

- **I connected to the toy's WiFi hotspot and can't access it at its gateway
  address (<http://192.168.0.1>)**: If you're on a cellular device, try turning
  off mobile data.
- **I connected my toy to my local network and can't access it over its
  `.local` URL**: Try turning your device's WiFi off and back on again. Some
  clients (like Flatpak apps on Linux) may not support connecting to devices
  this way. In that case, you can access the toy by its IP address or its WiFi
  hotspot.
- **I'm getting cryptic errors when building `esp-idf-sys`**: Try deleting the
  `./.embuild` directory and running `cargo clean` (**not** `just cargo
  clean`). Then try again.
