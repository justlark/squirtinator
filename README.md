# Squirtinator

The Squirtinator is a DIY sex toy that uses a peristaltic pump to deliver lube
through a tube at high velocity. It can be remotely controlled over WiFi via a
self-hosted web interface designed for mobile, and it supports both manual and
automatic modes of operation.

## Usage

Once you've assembled your Squirtinator and flashed the firmware, power it on
and connect to its WiFi network, called `SquirtinatorRemote` by default. You
may need to disconnect from cellular data.

You can navigate to http://192.168.71.1 to access the web interface.

## Hardware

This project uses the open hardware [Rust ESP development
board](https://github.com/esp-rs/esp-rust-board), based on the ESP32-C3
microcontroller.

## Prerequisites

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
