//! Telemetry initialization — 3-layer tracing subscriber.
//!
//! Layers:
//! 1. **fmt** — human-readable stderr (foreground) or rolling file (daemon).
//! 2. **JSON** — structured JSONL to `MANDO_LOG_DIR` or `{data_dir}/logs/daemon.jsonl`.
//! 3. **OTLP** — spans to Vector via gRPC (only with `otlp` feature + `OTEL_EXPORTER_OTLP_ENDPOINT`).

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

type BoxLayer = Box<dyn Layer<Registry> + Send + Sync>;

/// Build an `EnvFilter` from `RUST_LOG` (default `info`), with noisy transport
/// and telemetry crates suppressed regardless of the global level.
fn make_filter() -> EnvFilter {
    let base = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into());
    let filter = format!(
        "{base},\
         h2=off,\
         hyper_util=off,\
         reqwest=warn,\
         tonic=warn,\
         tower=warn,\
         opentelemetry_sdk=warn,\
         opentelemetry_otlp=warn,\
         opentelemetry-otlp=warn"
    );
    EnvFilter::new(filter)
}

/// Initialize the tracing subscriber.
///
/// - `foreground`: if true, fmt layer writes to stderr; otherwise to a rolling file.
pub fn init_tracing(foreground: bool) {
    let mut layers: Vec<BoxLayer> = Vec::new();

    // Layer 1: fmt (human-readable)
    // Each layer gets its own EnvFilter so filtering works correctly
    // (EnvFilter inside a Vec gets bypassed by other layers' Interest::always).
    if foreground {
        layers.push(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_filter(make_filter())
                .boxed(),
        );
    } else {
        let home = match std::env::var("HOME") {
            Ok(v) => v,
            Err(_) => {
                eprintln!("FATAL: HOME environment variable is not set");
                std::process::exit(1);
            }
        };
        let log_dir = std::path::PathBuf::from(home).join("Library/Logs/Mando");
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            eprintln!(
                "FATAL: cannot create log directory {}: {e}",
                log_dir.display()
            );
            std::process::exit(1);
        }
        let file_appender = tracing_appender::rolling::daily(&log_dir, "daemon.log");
        layers.push(
            tracing_subscriber::fmt::layer()
                .with_writer(file_appender)
                .with_ansi(false)
                .with_target(true)
                .with_filter(make_filter())
                .boxed(),
        );
    }

    // Layer 2: JSON file (always active)
    // MANDO_LOG_DIR overrides the default log directory (set by mando-dev for dev/sandbox).
    let json_dir = match std::env::var("MANDO_LOG_DIR") {
        Ok(dir) if !dir.is_empty() => std::path::PathBuf::from(dir),
        _ => global_infra::paths::data_dir().join("logs"),
    };
    if let Err(e) = std::fs::create_dir_all(&json_dir) {
        eprintln!(
            "FATAL: cannot create JSON log directory {}: {e}",
            json_dir.display()
        );
        std::process::exit(1);
    }
    let json_appender = tracing_appender::rolling::daily(&json_dir, "daemon.jsonl");
    layers.push(
        tracing_subscriber::fmt::layer()
            .json()
            .with_writer(json_appender)
            .with_target(true)
            .with_current_span(true)
            .with_span_list(true)
            .with_filter(make_filter())
            .boxed(),
    );

    // Layer 3: OTLP (dev only — requires `otlp` feature + env var)
    #[cfg(feature = "otlp")]
    if let Some(otel) = otlp::init_otel_layer() {
        layers.push(otel);
    }

    tracing_subscriber::registry().with(layers).init();
}

/// Flush and shut down the OTLP tracer provider (no-op without `otlp` feature).
pub fn shutdown_tracing() {
    #[cfg(feature = "otlp")]
    otlp::shutdown();
}

#[cfg(feature = "otlp")]
mod otlp {
    use std::sync::OnceLock;

    use opentelemetry::trace::TracerProvider;
    use opentelemetry_otlp::WithExportConfig;
    use tracing_subscriber::Layer;

    use super::BoxLayer;

    static TRACER_PROVIDER: OnceLock<opentelemetry_sdk::trace::SdkTracerProvider> = OnceLock::new();

    pub(super) fn shutdown() {
        if let Some(provider) = TRACER_PROVIDER.get() {
            if let Err(e) = provider.shutdown() {
                eprintln!("OTLP tracer shutdown error: {e}");
            }
        }
    }

    pub(super) fn init_otel_layer() -> Option<BoxLayer> {
        let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok()?;
        if endpoint.is_empty() {
            return None;
        }

        let exporter = match opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&endpoint)
            .build()
        {
            Ok(e) => e,
            Err(e) => {
                eprintln!("OTLP exporter build failed (endpoint={endpoint}): {e}");
                return None;
            }
        };

        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(
                opentelemetry_sdk::Resource::builder()
                    .with_service_name("mando-gw")
                    .build(),
            )
            .build();

        let tracer = provider.tracer("mando-gw");
        // Idempotent set: only the first call wins. Subsequent calls are
        // meaningless in practice (init runs once at startup) but the
        // return value is a `Result` we explicitly ignore here.
        match TRACER_PROVIDER.set(provider) {
            Ok(_) | Err(_) => {}
        }

        Some(tracing_opentelemetry::layer().with_tracer(tracer).boxed())
    }
}
