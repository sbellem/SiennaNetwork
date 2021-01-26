#!/usr/bin/env bash

Package=$1

# Container starts out as root, then switches to $USER:$GROUP if provided.
# If not provided, it continues to run as root, producing a root-owned binary.
USER=${USER:-0}
GROUP=${USER:-0}
groupadd -g $GROUP          $GROUP || true
useradd  -g $GROUP -u $USER $USER  || true

# The local registry is stored in a Docker volume mounted at /usr/local.
# This sure it is accessible to non-root users, which is the whole point:
mkdir -p /usr/local/cargo/registry
chown -R $USER /usr/local/cargo/registry

echo "Building $Package as $USER:$GROUP..."

# Execute a release build then optimize it with Binaryen
su $USER -c "\
  env RUSTFLAGS='-C link-arg=-s'                                 \
    cargo build -p $Package                                      \
    --release --target wasm32-unknown-unknown --locked --verbose \
    && wasm-opt -Oz                                              \
      ./target/wasm32-unknown-unknown/release/$Package.wasm      \
      -o ./contract.wasm                                         \
    && cat ./contract.wasm | gzip -n -9 > ./contract.wasm.gz     \
    && rm -f ./contract.wasm                                     \
  "
