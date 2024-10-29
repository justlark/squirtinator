image := "espressif/idf-rust:esp32c3_latest"

# run a cargo command
cargo +args:
  podman run --rm -it -v "$(pwd):/app:z" -w /app --userns keep-id {{image}} cargo {{args}}

flash: (cargo "build" "--release")
  espflash flash --monitor ./target/riscv32imc-esp-espidf/release/squirtinator
