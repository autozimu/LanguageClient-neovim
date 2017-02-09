#!/bin/sh

RUST_LOG=rls cargo run --manifest-path=/opt/rls/Cargo.toml 2>>/tmp/client.log

# tee -a /tmp/client.log | RUST_LOG=rls cargo run --manifest-path=/opt/rls/Cargo.toml 2>>/tmp/client.log | tee -a /tmp/client.log
