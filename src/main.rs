mod calendar;

use std::{collections::BTreeSet, env, net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    Router,
    extract::{Query, State, rejection::QueryRejection},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{Datelike, Utc};
use reqwest::{Client, Url, redirect::Policy};
use serde::Deserialize;
use tokio::{net::TcpListener, sync::Semaphore, time::timeout};

const DEFAULT_API: &str = "https://api.vacationplanner.ru";
const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone)]
struct App {
    client: Client,
    origin: Url,
    requests: Arc<Semaphore>,
}

// Deliberately no Debug implementation: query parameters contain credentials.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    token: Option<String>,
    years: Option<String>,
}

struct Error(StatusCode, &'static str);

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

impl Error {
    fn invalid(message: &'static str) -> Self {
        Self(StatusCode::BAD_REQUEST, message)
    }

    fn upstream() -> Self {
        Self(
            StatusCode::BAD_GATEWAY,
            "Vacationplanner returned an invalid response",
        )
    }
}

fn origin(value: &str) -> Result<Url, Error> {
    let url = Url::parse(value).map_err(|_| Error::invalid("url must be an API origin"))?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::invalid(
            "url must contain only a scheme, host and optional port",
        ));
    }
    Ok(url)
}

fn years(parameters: &Parameters) -> Result<BTreeSet<i32>, Error> {
    if let Some(value) = parameters.years.as_ref() {
        let years: BTreeSet<i32> = value
            .split(',')
            .map(|year| {
                year.trim()
                    .parse::<i32>()
                    .ok()
                    .filter(|year| (1..=9998).contains(year))
                    .ok_or_else(|| {
                        Error::invalid("years must be comma-separated numbers from 1 to 9998")
                    })
            })
            .collect::<Result<_, _>>()?;
        if years.len() > 10 {
            return Err(Error::invalid("at most 10 distinct years are supported"));
        }
        Ok(years)
    } else {
        let current = Utc::now().year();
        Ok(BTreeSet::from([current, current + 1]))
    }
}

fn authorization(parameters: &Parameters) -> Result<HeaderValue, Error> {
    let token = parameters.token.as_deref();
    let token = token
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 4096
                && value.bytes().all(|byte| byte.is_ascii_graphic())
        })
        .ok_or(Error(
            StatusCode::UNAUTHORIZED,
            "a nonempty API token is required",
        ))?;
    let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| Error::invalid("invalid token"))?;
    value.set_sensitive(true);
    Ok(value)
}

async fn fetch_year(
    app: &App,
    base: &Url,
    auth: &HeaderValue,
    year: i32,
) -> Result<Vec<calendar::Vacation>, Error> {
    let mut url = base.clone();
    url.set_path(&format!("/v1/vacations/year/{year}"));
    let mut response = app
        .client
        .get(url)
        .header(header::AUTHORIZATION, auth.clone())
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(upstream_error)?;
    match response.status() {
        StatusCode::OK => {}
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            return Err(Error(
                response.status(),
                "Vacationplanner rejected the API token",
            ));
        }
        StatusCode::TOO_MANY_REQUESTS => {
            return Err(Error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Vacationplanner rate limit reached",
            ));
        }
        _ => return Err(Error::upstream()),
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err(Error::upstream());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(upstream_error)? {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(Error::upstream());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| Error::upstream())
}

fn upstream_error(error: reqwest::Error) -> Error {
    // Never expose reqwest errors: they can contain upstream URLs or response data.
    if error.is_timeout() {
        Error(
            StatusCode::GATEWAY_TIMEOUT,
            "Vacationplanner request timed out",
        )
    } else {
        Error::upstream()
    }
}

async fn calendar(
    State(app): State<App>,
    parameters: Result<Query<Parameters>, QueryRejection>,
) -> Result<Response, Error> {
    let Query(parameters) = parameters.map_err(|_| Error::invalid("invalid query parameters"))?;
    let auth = authorization(&parameters)?;
    let years = years(&parameters)?;
    let _permit = app.requests.try_acquire().map_err(|_| {
        Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "service is busy; retry later",
        )
    })?;
    let body = timeout(Duration::from_secs(60), async {
        let mut vacations = Vec::new();
        for year in years {
            vacations.extend(fetch_year(&app, &app.origin, &auth, year).await?);
        }
        calendar::render(vacations, &app.origin)
    })
    .await
    .map_err(|_| Error(StatusCode::GATEWAY_TIMEOUT, "calendar request timed out"))??;
    Ok((
        [
            (header::CONTENT_TYPE, "text/calendar; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "inline; filename=vacations.ics",
            ),
        ],
        body,
    )
        .into_response())
}

async fn private_response(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

async fn shutdown() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = interrupt => {}, _ = terminate => {} }
}

async fn run() -> Result<(), &'static str> {
    let bind: SocketAddr = env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()
        .map_err(|_| "invalid BIND_ADDR")?;
    let timeout_seconds: u64 = env::var("UPSTREAM_TIMEOUT_SECONDS")
        .unwrap_or_else(|_| "15".into())
        .parse()
        .ok()
        .filter(|seconds| (1..=60).contains(seconds))
        .ok_or("UPSTREAM_TIMEOUT_SECONDS must be between 1 and 60")?;
    let origin = origin(&env::var("API_URL").unwrap_or_else(|_| DEFAULT_API.into()))
        .map_err(|_| "API_URL must contain only a scheme, host and optional port")?;
    let client = Client::builder()
        .redirect(Policy::none())
        .referer(false)
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(timeout_seconds))
        .user_agent(concat!("vacationplanner2ics/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "could not initialize HTTP client")?;
    let app = App {
        client,
        origin,
        requests: Arc::new(Semaphore::new(16)),
    };
    let router = Router::new()
        .route("/calendar.ics", get(calendar))
        .route("/healthz", get(|| async { "ok" }))
        .layer(middleware::from_fn(private_response))
        .with_state(app);
    let listener = TcpListener::bind(bind)
        .await
        .map_err(|_| "could not bind BIND_ADDR")?;
    eprintln!("vacationplanner2ics listening on {bind}");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown())
        .await
        .map_err(|_| "HTTP server failed")
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            std::process::ExitCode::FAILURE
        }
    }
}
