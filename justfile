image := "squirtinator"

# build the dev container image
build:
  podman build -t {{image}} .

# run a cargo command
cargo +args: build
  podman run --rm -it {{image}} cargo {{args}}
