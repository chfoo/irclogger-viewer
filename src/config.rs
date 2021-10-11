use std::path::PathBuf;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub chat_log_directory: PathBuf, // Directory containing channel-named directories
    pub apache_password_file: PathBuf, // Password file in htpasswd format,
    pub custom_message_html_file: PathBuf,
    pub web_server_port_number: u16,
}
