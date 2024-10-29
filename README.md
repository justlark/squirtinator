# Squirtinator

## Prerequisites

To flash the firmware, you'll need to install:

- [just](https://github.com/casey/just?tab=readme-ov-file#installation)
- [espflash](https://github.com/esp-rs/espflash/tree/main/espflash#installation)

If you're on Linux, make sure your user has the necessary permissions to read
and write the serial port over USB. See
[here](https://docs.espressif.com/projects/esp-idf/en/latest/esp32/get-started/establish-serial-connection.html#linux-dialout-group)
for instructions.

## Flashing

To build and flash the firmware with release optimizations, run:

```sh
just flash
```

## Development

It's easiest to build the firmware in a container. You can run any Cargo
command in the container with `just`, like this:

```sh
just cargo check
```
