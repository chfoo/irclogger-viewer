use encoding_rs_io::DecodeReaderBytesBuilder;
use lazy_static::lazy_static;
use regex::Regex;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use chrono::{Date, DateTime, NaiveTime, Utc};

pub struct LogLine {
    pub date: DateTime<Utc>,
    pub content: LogLineContent,
}

pub enum LogLineContent {
    Message { nickname: String, text: String },
    Status(String),
}

pub fn count_message_lines(path: &Path, _log_date: &Date<Utc>) -> anyhow::Result<u64> {
    let mut count = 0;

    let file = File::open(path)?;
    let file = DecodeReaderBytesBuilder::new()
        .encoding(Some(encoding_rs::UTF_8))
        .build(file);
    let file = BufReader::new(file);

    for raw_line in file.lines() {
        let line = raw_line?;

        if !line.contains("] *** ") {
            count += 1;
        }
    }

    Ok(count)
}

pub fn read_lines(path: &Path, log_date: &Date<Utc>) -> anyhow::Result<Vec<LogLine>> {
    let file = File::open(path)?;
    let file = DecodeReaderBytesBuilder::new()
        .encoding(Some(encoding_rs::UTF_8))
        .build(file);
    let file = BufReader::new(file);
    let mut lines = Vec::new();

    for raw_line in file.lines() {
        let line = raw_line?;

        if line.is_empty() {
            continue;
        }

        let line = parse_line(line, log_date)?;
        lines.push(line)
    }

    Ok(lines)
}

fn parse_line(line: String, log_date: &Date<Utc>) -> anyhow::Result<LogLine> {
    lazy_static! {
        static ref PATTERN: Regex = Regex::new(r"\[(\d\d:\d\d)\] (\S+) (.*)").unwrap();
    }

    if let Some(captures) = PATTERN.captures(&line) {
        let time_str = captures.get(1).unwrap().as_str();
        let nickname = captures
            .get(2)
            .unwrap()
            .as_str()
            .trim_start_matches('<')
            .trim_end_matches('>');
        let text = captures.get(3).unwrap().as_str();

        let time = NaiveTime::parse_from_str(time_str, "%H:%M")?;
        let date = log_date.and_time(time).unwrap();

        if nickname == "***" {
            Ok(LogLine {
                date,
                content: LogLineContent::Status(text.to_string()),
            })
        } else {
            Ok(LogLine {
                date,
                content: LogLineContent::Message {
                    nickname: nickname.to_string(),
                    text: text.to_string(),
                },
            })
        }
    } else {
        anyhow::bail!("Parse line error: {}", line);
    }
}
