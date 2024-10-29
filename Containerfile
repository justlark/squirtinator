FROM espressif/idf-rust:esp32c3_latest
WORKDIR /home/esp
COPY ./src/ ./src/
COPY ./Cargo.toml .
COPY ./Cargo.lock .
COPY ./build.rs .
COPY ./rust-toolchain.toml .
COPY ./sdkconfig.defaults .
COPY ./.cargo/config.toml ./.cargo/
RUN cargo build --bin dummy
