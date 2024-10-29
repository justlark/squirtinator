FROM espressif/idf-rust:esp32c3_latest
WORKDIR /home/esp
COPY ./src ./Cargo.toml ./Cargo.lock ./.cargo ./build.rs ./rust-toolchain.toml ./sdkconfig.defaults ./
