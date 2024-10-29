image := "espressif/idf-rust:esp32c3_latest"
bin := "./target/riscv32imc-esp-espidf/release/squirtinator"

# run a cargo command
cargo +args:
  podman run --rm -it -v "$(pwd):/app:z" -w /app --userns keep-id {{image}} cargo {{args}}

# flash the firmware and monitor the logs
dev: (cargo "build" "--release")
  espflash flash --monitor {{bin}}

# flash the firmware
flash: (cargo "build" "--release")
  espflash flash {{bin}}
