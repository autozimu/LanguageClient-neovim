#!/bin/sh
# Tee server stdio into log file.

LOG=/tmp/LanguageClient.log

# tee -a $LOG | RUST_LOG=rls cargo run --release --manifest-path=/opt/rls/Cargo.toml 2>>$LOG | tee -a $LOG

# tee -a $LOG | php /user/local/bin/php-language-server.php | tee -a $LOG

# tee -a $LOG | /usr/local/bin/language-server-stdio.js | tee -a $LOG

# tee -a $LOG | pyls 2>>$LOG | tee -a $LOG
