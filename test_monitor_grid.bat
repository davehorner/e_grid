@echo off
echo Testing monitor grid display...
cd /d c:\w\demos\e_midi\e_grid
timeout /t 2 /nobreak
cargo run --bin e_grid server
