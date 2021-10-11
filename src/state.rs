use std::{
    ffi::OsStr,
    io::{BufRead, BufReader, Cursor},
    path::PathBuf,
};

use chrono::{Date, NaiveDate, Utc};
use encoding_rs_io::DecodeReaderBytesBuilder;
use gotham_derive::StateData;

use crate::reader::LogLine;

pub struct ChannelInfo {
    pub name: String,
    pub is_private: bool,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct ChannelDailyEntry {
    pub date_slug: String,
    pub date: Date<Utc>,
    pub message_count: u64,
}

#[derive(Clone, StateData)]
pub struct AppState {
    pub chat_log_directory: PathBuf,
    pub apache_password_file: PathBuf,
    pub custom_message_html_file: PathBuf,
}

pub struct SearchResultEntry {
    pub date_slug: String,
    pub line_number: u64,
    pub raw_line: String,
}

impl AppState {
    pub fn get_channels(&self) -> anyhow::Result<Vec<ChannelInfo>> {
        let mut channels = Vec::new();
        let dirs = std::fs::read_dir(&self.chat_log_directory)?;

        for entry in dirs {
            let entry = entry?;
            if entry.metadata()?.is_dir() {
                if let Ok(filename) = entry.file_name().into_string() {
                    channels.push(ChannelInfo {
                        is_private: self.is_channel_private(&filename)?,
                        name: filename,
                    });
                }
            }
        }

        channels.sort_unstable_by(|a, b| a.name.cmp(&b.name));

        Ok(channels)
    }

    pub fn is_channel_private(&self, name: &str) -> anyhow::Result<bool> {
        Ok(!self.is_channel_marked_public(name) && self.is_channel_in_password_file(name)?)
    }

    pub fn is_channel_marked_public(&self, name: &str) -> bool {
        let public_path = self.chat_log_directory.join(name).join("PUBLIC");

        public_path.is_file()
    }

    pub fn is_channel_in_password_file(&self, name: &str) -> anyhow::Result<bool> {
        let content = std::fs::read_to_string(&self.apache_password_file)?;

        for line in content.split('\n') {
            if line.starts_with('#') {
                // Despite the bash script saving both unprefixed and prefixed
                // channel names, it's ultimately treated as a comment...
                continue;
            } else if let Some((candidate_name, _)) = line.split_once(":") {
                if name == candidate_name {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    pub fn is_password_ok(&self, channel_name: &str, password: &str) -> anyhow::Result<bool> {
        let passwords = std::fs::read_to_string(&self.apache_password_file)?;
        let passwords = htpasswd_verify::load(&passwords);

        Ok(passwords.check(channel_name, password))
    }

    pub fn get_channel_daily_entries(&self, name: &str) -> anyhow::Result<Vec<ChannelDailyEntry>> {
        let mut channel_entries = Vec::new();

        for date_slug in self.get_channel_log_date_slugs(name)? {
            let date = parse_date_slug(&date_slug)?;
            let log_path = self.get_log_path(name, &date_slug)?;
            let message_count = crate::reader::count_message_lines(&log_path, &date)?;

            channel_entries.push(ChannelDailyEntry {
                date,
                date_slug,
                message_count,
            });
        }

        channel_entries.sort_unstable();
        channel_entries.reverse();

        Ok(channel_entries)
    }

    fn get_channel_log_date_slugs(&self, name: &str) -> anyhow::Result<Vec<String>> {
        let channel_dir = self.chat_log_directory.join(name);
        let mut date_slugs = Vec::new();

        for entry in std::fs::read_dir(channel_dir)? {
            let entry = entry?;

            if let Some("log") = entry.path().extension().and_then(OsStr::to_str) {
                let date_slug = entry
                    .path()
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                date_slugs.push(date_slug)
            }
        }

        date_slugs.sort_unstable();
        date_slugs.reverse();

        Ok(date_slugs)
    }

    pub fn get_raw_log(&self, name: &str, date_slug: &str) -> anyhow::Result<Vec<u8>> {
        let log_path = self.get_log_path(name, date_slug)?;

        Ok(std::fs::read(log_path)?)
    }

    pub fn get_log_lines(&self, name: &str, date_slug: &str) -> anyhow::Result<Vec<LogLine>> {
        let date = parse_date_slug(date_slug)?;
        let log_path = self.get_log_path(name, date_slug)?;

        crate::reader::read_lines(&log_path, &date)
    }

    fn get_log_path(&self, name: &str, date_slug: &str) -> anyhow::Result<PathBuf> {
        let log_path = self
            .chat_log_directory
            .join(name)
            .join(format!("{}.log", date_slug));

        Ok(log_path)
    }

    pub fn get_custom_message(&self) -> anyhow::Result<String> {
        Ok(std::fs::read_to_string(&self.custom_message_html_file)?)
    }

    pub fn search_channel(
        &self,
        channel_name: &str,
        query: &str,
        case_sensitive: bool,
        verbatim: bool,
        whole_word: bool,
    ) -> anyhow::Result<Vec<SearchResultEntry>> {
        let channel_dir = self.chat_log_directory.join(channel_name);
        let date_slugs = self.get_channel_log_date_slugs(channel_name)?;
        let log_files = date_slugs
            .iter()
            .map(|slug| channel_dir.join(format!("{}.log", slug)))
            .collect::<Vec<PathBuf>>();

        let mut process = std::process::Command::new("timeout");
        process.arg("10s").arg("agrep");

        if !case_sensitive {
            process.arg("-i0");
        }

        if verbatim {
            process.arg("-k");
        }

        if whole_word {
            process.arg("-w");
        }

        process.arg("-n").arg(query);

        for path in log_files {
            process.arg(path);
        }

        let output = process.output()?;
        let output = DecodeReaderBytesBuilder::new()
            .encoding(Some(encoding_rs::UTF_8))
            .build(Cursor::new(output.stdout));
        let output = BufReader::new(output);
        let mut search_results = Vec::new();

        for (count, line) in output.lines().enumerate() {
            if count == 10000 {
                search_results.push(SearchResultEntry {
                    date_slug: String::new(),
                    line_number: 0,
                    raw_line: "(max search results exceed)".to_string(),
                });
                break;
            }

            let line = line?;
            let parts = line.splitn(3, ':');
            let parts = parts.collect::<Vec<&str>>();

            if parts.len() == 3 {
                let file_path = parts[0];
                let line_number = parts[1].trim().parse::<u64>()?;
                let raw_line = parts[2];
                let file_path = PathBuf::from(file_path);
                let date_slug = file_path.file_stem().unwrap_or_default().to_string_lossy();

                search_results.push(SearchResultEntry {
                    date_slug: date_slug.to_string(),
                    line_number,
                    raw_line: raw_line.to_string(),
                });
            }
        }

        Ok(search_results)
    }
}

fn parse_date_slug(date_slug: &str) -> anyhow::Result<Date<Utc>> {
    let date_string = date_slug.split_once(",").unwrap().0;
    Ok(Date::from_utc(
        NaiveDate::parse_from_str(date_string, "%Y-%m-%d")?,
        Utc,
    ))
}
