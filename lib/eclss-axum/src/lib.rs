pub use axum;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use eclss::{sensor::Registry, Eclss, SensorMetrics};
use std::sync::Arc;

#[derive(Clone)]
struct AppState<const SENSORS: usize> {
    metrics: &'static SensorMetrics,
    sensors: &'static Registry<SENSORS>,
    location: Option<Arc<str>>,
}

pub fn app<I, const SENSORS: usize>(
    eclss: &'static Eclss<I, { SENSORS }>,
    location: Option<Arc<str>>,
) -> Router {
    Router::new()
        .route("/metrics", get(get_metrics))
        .route("/metrics.json", get(get_metrics_json))
        .route("/sensors.json", get(get_sensors))
        .route("/", get(index))
        .with_state(AppState {
            metrics: eclss.metrics(),
            sensors: eclss.sensors(),
            location,
        })
        .fallback(not_found)
}

async fn get_metrics<const SENSORS: usize>(
    State(AppState { metrics, .. }): State<AppState<{ SENSORS }>>,
) -> String {
    let mut resp = String::new();
    metrics.fmt_metrics(&mut resp).unwrap();
    resp
}

#[derive(serde::Serialize)]
struct MetricsResponse {
    #[serde(flatten)]
    metrics: &'static SensorMetrics,
    location: Option<Arc<str>>,
}

async fn get_metrics_json<const SENSORS: usize>(
    State(AppState {
        metrics, location, ..
    }): State<AppState<{ SENSORS }>>,
) -> Json<MetricsResponse> {
    Json(MetricsResponse {
        metrics,
        location: location.clone(),
    })
}

async fn get_sensors<const SENSORS: usize>(
    State(AppState { sensors, .. }): State<AppState<{ SENSORS }>>,
) -> Json<&'static Registry<{ SENSORS }>> {
    Json(sensors)
}

async fn index() -> Html<&'static str> {
    Html(
        "<!DOCTYPE html>\
        <html>\
        <head>\
            <title>ECLSS</title>\
        </head>\
        <body>\
            <h1>ECLSS</h1>\
            <ul>\
                <li><a href=\"/metrics\">Metrics (Prometheus)</a></li>\
                <li><a href=\"/metrics.json\">Metrics (JSON)</a></li>\
                <li><a href=\"/sensors.json\">Sensors (JSON)</a></li>\
            </ul>\
        </body>\
        </html>",
    )
}

async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "can't get ye flask")
}
