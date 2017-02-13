#!/bin/sh

RUST_LOG=rls cargo run --manifest-path=/opt/rls/Cargo.toml 2>>/tmp/client.log

# tee -a /tmp/client.log | RUST_LOG=rls cargo run --manifest-path=/opt/rls/Cargo.toml 2>>/tmp/client.log | tee -a /tmp/client.log

# tee -a /tmp/LanguageClient.log | php $HOME/.config/composer/vendor/felixfbecker/language-server/bin/php-language-server.php | tee -a /tmp/LanguageClient.log
