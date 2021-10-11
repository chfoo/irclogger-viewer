# irclogger-viewer

This is a simple Rust web server that mostly replaces the [irclogger](https://colas.nahaboo.net/Code/IrcLogger) Bash scripts for viewing IRC logs. A replacement was needed because the original has security issues.

If you want to use this as well, note that you may need to change some of the code since the URLs are hardcoded.

To build the app, install Rust and run the command `cargo build --release`.
