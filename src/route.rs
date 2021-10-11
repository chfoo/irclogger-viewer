use askama::Template;
use chrono::{DateTime, Utc};
use gotham::{
    helpers::http::response::{create_empty_response, create_response},
    hyper::{Body, HeaderMap, Response, StatusCode, Uri},
    state::{FromState, State},
};
use gotham_derive::{StateData, StaticResponseExtender};
use http_auth_basic::Credentials;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;

use crate::{
    reader::{LogLine, LogLineContent},
    state::{AppState, ChannelDailyEntry, ChannelInfo, SearchResultEntry},
};

fn render_template<T: Template>(state: &mut State, template: T) -> anyhow::Result<Response<Body>> {
    let content = template.render()?;

    Ok(create_response(
        state,
        StatusCode::OK,
        mime::TEXT_HTML_UTF_8,
        content.into_bytes(),
    ))
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
pub struct ChannelParams {
    channel: String,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    channels: Vec<ChannelInfo>,
    message: String,
}

pub fn index(state: &mut State) -> anyhow::Result<Response<Body>> {
    let app_state = AppState::borrow_from(state);
    let channels = app_state.get_channels()?;
    let message = app_state.get_custom_message()?;

    let template = IndexTemplate { channels, message };
    let response = render_template(state, template)?;

    Ok(response)
}

#[derive(Template)]
#[template(path = "channel_index.html")]
struct ChannelIndexTemplate {
    channel_name: String,
    entries: Vec<ChannelDailyEntry>,
}

pub fn channel_daily_index(state: &mut State) -> anyhow::Result<Response<Body>> {
    let params = ChannelParams::take_from(state);

    if !user_has_access(state, &params.channel)? {
        return Ok(build_auth_response(state));
    }

    let app_state = AppState::borrow_from(state);
    let entries = app_state.get_channel_daily_entries(&params.channel)?;

    let template = ChannelIndexTemplate {
        channel_name: params.channel,
        entries,
    };
    let response = render_template(state, template)?;

    Ok(response)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
pub struct ChannelLinesQuery {
    pub date: String,
    sel: Option<String>,
    raw: Option<String>,
}

#[derive(Template)]
#[template(path = "channel_lines.html")]
struct ChannelLinesTemplate {
    pub channel_name: String,
    pub lines: Vec<LogOutputLine>,
    pub date_slug: String,
    pub selected_line_number: u64,
}

struct LogOutputLine {
    pub date: DateTime<Utc>,
    pub nickname: String,
    pub text: String,
    pub line_number: u64,
}

pub fn channel_lines(state: &mut State) -> anyhow::Result<Response<Body>> {
    let params = ChannelParams::take_from(state);

    if !user_has_access(state, &params.channel)? {
        return Ok(build_auth_response(state));
    }

    let query = ChannelLinesQuery::take_from(state);

    if !is_date_string_ok(&query.date) {
        return Ok(create_empty_response(state, StatusCode::BAD_REQUEST));
    }

    let app_state = AppState::borrow_from(state);

    if let Some("on") = query.raw.as_deref() {
        let response = create_response(
            state,
            StatusCode::OK,
            mime::TEXT_PLAIN_UTF_8,
            app_state.get_raw_log(&params.channel, &query.date)?,
        );

        return Ok(response);
    }

    let lines = app_state.get_log_lines(&params.channel, &query.date)?;
    let lines = make_output_lines(&lines);

    let template = ChannelLinesTemplate {
        channel_name: params.channel.clone(),
        lines,
        date_slug: query.date.clone(),
        selected_line_number: query
            .sel
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(u64::MAX),
    };

    let mut response = render_template(state, template)?;
    let headers = HeaderMap::borrow_from(state);
    let host = match headers.get("host") {
        Some(host) => host.to_str().unwrap(),
        None => "",
    };

    response.headers_mut().append(
        "Link",
        format!(
            "<https://{host}/bin/irclogger_logs/{channel}/?date={date_slug}>; rel=\"canonical\"",
            host = host,
            channel = params.channel,
            date_slug = query.date
        )
        .parse()
        .unwrap(),
    );

    Ok(response)
}

fn is_date_string_ok(date: &str) -> bool {
    lazy_static! {
        static ref PATTERN: Regex = Regex::new(r"^\d\d\d\d-\d\d-\d\d,\w+$").unwrap();
    }

    PATTERN.is_match(date)
}

fn make_output_lines(lines: &[LogLine]) -> Vec<LogOutputLine> {
    let mut output_lines = Vec::new();

    for (line_number, line) in lines.iter().enumerate() {
        let line_number = line_number as u64 + 1;
        let output_line = match &line.content {
            LogLineContent::Status(text) => LogOutputLine {
                date: line.date,
                nickname: String::new(),
                text: text.clone(),
                line_number,
            },
            LogLineContent::Message { nickname, text } => LogOutputLine {
                date: line.date,
                nickname: nickname.clone(),
                text: text.clone(),
                line_number,
            },
        };

        output_lines.push(output_line);
    }

    output_lines
}

#[derive(Template)]
#[template(path = "channel_search.html")]
struct ChannelSearchTemplate {
    pub channel_name: String,
    pub has_results: bool,
    pub results: Vec<SearchResultEntry>,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
pub struct ChannelSearchQuery {
    search: Option<String>,
    //action: Option<String>,
    case: Option<String>,
    verbatim: Option<String>,
    word: Option<String>,
}

pub fn channel_search(state: &mut State) -> anyhow::Result<Response<Body>> {
    let params = ChannelParams::take_from(state);

    if !user_has_access(state, &params.channel)? {
        return Ok(build_auth_response(state));
    }

    let query = ChannelSearchQuery::take_from(state);
    let app_state = AppState::borrow_from(state);

    let search_results = if query.search.is_some() {
        app_state.search_channel(
            &params.channel,
            query.search.as_deref().unwrap_or_default(),
            query.case.unwrap_or_default() == "on",
            query.verbatim.unwrap_or_default() == "on",
            query.word.unwrap_or_default() == "on",
        )?
    } else {
        Vec::new()
    };

    let template = ChannelSearchTemplate {
        channel_name: params.channel.clone(),
        has_results: query.search.is_some(),
        results: search_results,
    };

    let response = render_template(state, template)?;

    Ok(response)
}

pub fn redirect_channel_daily_index(state: &mut State) -> anyhow::Result<Response<Body>> {
    let params = ChannelParams::borrow_from(state);
    let mut response = create_empty_response(state, StatusCode::TEMPORARY_REDIRECT);

    response.headers_mut().insert(
        "Location",
        format!("/bin/irclogger_logs/{}", params.channel,).parse()?,
    );

    Ok(response)
}

pub fn redirect_channel_lines(state: &mut State) -> anyhow::Result<Response<Body>> {
    let uri = state.borrow::<Uri>();
    let params = ChannelParams::borrow_from(state);
    let mut response = create_empty_response(state, StatusCode::TEMPORARY_REDIRECT);

    response.headers_mut().insert(
        "Location",
        format!(
            "/bin/irclogger_log/{}/?{}",
            params.channel,
            uri.query().unwrap_or_default()
        )
        .parse()?,
    );

    Ok(response)
}

pub fn redirect_channel_search(state: &mut State) -> anyhow::Result<Response<Body>> {
    let uri = state.borrow::<Uri>();
    let params = ChannelParams::borrow_from(state);
    let mut response = create_empty_response(state, StatusCode::TEMPORARY_REDIRECT);

    response.headers_mut().insert(
        "Location",
        format!(
            "/bin/irclogger_log_search/{}/?{}",
            params.channel,
            uri.query().unwrap_or_default()
        )
        .parse()?,
    );

    Ok(response)
}

fn user_has_access(state: &mut State, channel: &str) -> anyhow::Result<bool> {
    let app_state = AppState::borrow_from(state);

    if app_state.is_channel_private(channel)? {
        let headers = state.borrow::<HeaderMap>();

        if let Some(value) = headers.get("Authorization") {
            match Credentials::from_header(value.to_str().unwrap_or_default().to_string()) {
                Ok(credentials) => Ok(channel == credentials.user_id
                    && app_state.is_password_ok(channel, &credentials.password)?),
                Err(_) => Ok(false),
            }
        } else {
            Ok(false)
        }
    } else {
        Ok(true)
    }
}

fn build_auth_response(state: &mut State) -> Response<Body> {
    let mut response = create_response(
        state,
        StatusCode::UNAUTHORIZED,
        mime::TEXT_PLAIN_UTF_8,
        "These logs are not public. See the homepage for details. The username is the channel name lowercase and without the hash symbol.",
    );
    response.headers_mut().insert(
        "WWW-Authenticate",
        "Basic realm=\"irclogger-viewer\", charset=\"UTF-8\""
            .parse()
            .unwrap(),
    );

    response
}
