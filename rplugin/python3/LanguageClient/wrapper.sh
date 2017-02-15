#!/bin/sh
# Tee server stdio into log file.

# tee -a /tmp/server.log | RUST_LOG=rls cargo run --manifest-path=/opt/rls/Cargo.toml 2>>/tmp/server.log | tee -a /tmp/server.log

# tee -a /tmp/server.log | php /user/local/bin/php-language-server.php | tee -a /tmp/server.log

# tee -a /tmp/server.log | /usr/local/bin/language-server-stdio.js | tee -a /tmp/server.log
