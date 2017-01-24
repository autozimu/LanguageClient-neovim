#!/bin/sh

tee -a /tmp/rls.log | cargo run --manifest-path=/opt/rls/Cargo.toml | tee -a /tmp/rls.log
