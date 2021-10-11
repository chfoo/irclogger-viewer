use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use gotham::{
    handler::IntoResponse,
    helpers::http::response::create_empty_response,
    hyper::{Body, Response, StatusCode},
    middleware::state::StateMiddleware,
    pipeline::{single::single_pipeline, single_middleware},
    router::{
        builder::{build_router, DefineSingleRoute, DrawRoutes},
        Router,
    },
    state::State,
};

use crate::{config::Config, state::AppState};

#[tokio::main]
pub async fn run(config: Config) -> anyhow::Result<()> {
    let addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        config.web_server_port_number,
    );
    gotham::init_server(addr, build_routes(&config))
        .await
        .map_err(|_| anyhow::anyhow!("Couldn't start server"))?;

    Ok(())
}

fn build_routes(config: &Config) -> Router {
    let app_state = AppState {
        chat_log_directory: config.chat_log_directory.clone(),
        apache_password_file: config.apache_password_file.clone(),
        custom_message_html_file: config.custom_message_html_file.clone(),
    };

    let middleware = StateMiddleware::new(app_state);
    let pipeline = single_middleware(middleware);
    let (chain, pipelines) = single_pipeline(pipeline);

    build_router(chain, pipelines, |route| {
        route
            .get("/bin/irclogger_logs")
            .to(|state| error_wrapper(state, crate::route::index));
        route
            .get("/bin/irclogger_logs/:channel:[a-z0-9._-]+")
            .with_path_extractor::<crate::route::ChannelParams>()
            .to(|state| error_wrapper(state, crate::route::channel_daily_index));
        route
            .get("/bin/irclogger_log/:channel:[a-z0-9._-]+")
            .with_path_extractor::<crate::route::ChannelParams>()
            .with_query_string_extractor::<crate::route::ChannelLinesQuery>()
            .to(|state| error_wrapper(state, crate::route::channel_lines));
        route
            .get("/bin/irclogger_log_search/:channel:[a-z0-9._-]+")
            .with_path_extractor::<crate::route::ChannelParams>()
            .with_query_string_extractor::<crate::route::ChannelSearchQuery>()
            .to(|state| error_wrapper(state, crate::route::channel_search));
        route
            .get("bin/irclogger_logs_a/:channel:[a-z0-9._-]+")
            .with_path_extractor::<crate::route::ChannelParams>()
            .to(|state| error_wrapper(state, crate::route::redirect_channel_daily_index));
        route
            .get("bin/irclogger_log_a/:channel:[a-z0-9._-]+")
            .with_path_extractor::<crate::route::ChannelParams>()
            .to(|state| error_wrapper(state, crate::route::redirect_channel_lines));
        route
            .get("bin/irclogger_log_search_a/:channel:[a-z0-9._-]+")
            .with_path_extractor::<crate::route::ChannelParams>()
            .to(|state| error_wrapper(state, crate::route::redirect_channel_search));
    })
}

fn error_wrapper<F, R>(mut state: State, func: F) -> (State, Response<Body>)
where
    F: FnOnce(&mut State) -> anyhow::Result<R>,
    R: IntoResponse,
{
    let response = match func(&mut state) {
        Ok(response) => response.into_response(&state),
        Err(error) => {
            dbg!(error);
            create_empty_response(&state, StatusCode::INTERNAL_SERVER_ERROR)
        }
    };

    (state, response)
}
