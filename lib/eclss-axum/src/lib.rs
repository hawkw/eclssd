pub use axum;
use axum::{extract::State, routing::get, Json, Router};
use eclss::{sensor::Registry, Eclss, SensorMetrics};

#[derive(Clone)]
struct AppState<const SENSORS: usize> {
    metrics: &'static SensorMetrics,
    sensors: &'static Registry<SENSORS>,
}

pub fn app<I, const SENSORS: usize>(eclss: &'static Eclss<I, { SENSORS }>) -> Router {
    Router::new()
        .route("/metrics", get(get_metrics))
        .route("/sensors.json", get(get_sensors))
        .with_state(AppState {
            metrics: eclss.metrics(),
            sensors: eclss.sensors(),
        })
}

async fn get_metrics<const SENSORS: usize>(
    State(AppState { metrics, .. }): State<AppState<{ SENSORS }>>,
) -> String {
    let mut resp = String::new();
    metrics.fmt_metrics(&mut resp).unwrap();
    resp
}

async fn get_sensors<const SENSORS: usize>(
    State(AppState { sensors, .. }): State<AppState<{ SENSORS }>>,
) -> Json<&'static Registry<{ SENSORS }>> {
    Json(sensors)
}
