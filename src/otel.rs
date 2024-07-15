//! copied from containerd/runwasi

use std::env;

use opentelemetry::trace::TraceError;
use opentelemetry_otlp::{
    Protocol, SpanExporterBuilder, WithExportConfig, OTEL_EXPORTER_OTLP_PROTOCOL_DEFAULT,
};
pub use opentelemetry_otlp::{
    OTEL_EXPORTER_OTLP_ENDPOINT, OTEL_EXPORTER_OTLP_PROTOCOL, OTEL_EXPORTER_OTLP_TRACES_ENDPOINT,
};
use opentelemetry_sdk::{runtime, trace as sdktrace};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::{EnvFilter, Registry};

const OTEL_EXPORTER_OTLP_PROTOCOL_HTTP_PROTOBUF: &str = "http/protobuf";
const OTEL_EXPORTER_OTLP_PROTOCOL_GRPC: &str = "grpc";
const OTEL_EXPORTER_OTLP_TRACES_PROTOCOL: &str = "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL";
const OTEL_SDK_DISABLED: &str = "OTEL_SDK_DISABLED";

/// Configuration struct for OpenTelemetry setup.
pub struct Config {
    traces_endpoint: String,
    traces_protocol: Protocol,
}

/// Returns `true` if traces are enabled, `false` otherwise.
///
/// Traces are enabled if either `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` or `OTEL_EXPORTER_OTLP_ENDPOINT` is set and not empty.
/// `OTEL_SDK_DISABLED` can be set to `true` to disable traces.
pub fn traces_enabled() -> bool {
    let check_env_var = |var: &str| env::var_os(var).is_some_and(|val| !val.is_empty());
    let traces_endpoint = check_env_var(OTEL_EXPORTER_OTLP_TRACES_ENDPOINT);
    let otlp_endpoint = check_env_var(OTEL_EXPORTER_OTLP_ENDPOINT);

    // https://opentelemetry.io/docs/specs/otel/configuration/sdk-environment-variables/#general-sdk-configuration
    let sdk_disabled = env::var_os(OTEL_SDK_DISABLED).is_some_and(|val| val == "true");
    (traces_endpoint || otlp_endpoint) && !sdk_disabled
}

/// Initializes a new OpenTelemetry tracer with the OTLP exporter.
///
/// Returns a `Result` containing the initialized tracer or a `TraceError` if initialization fails.
///
/// https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/protocol/exporter.md#configuration-options
impl Config {
    pub fn build_from_env() -> anyhow::Result<Self> {
        let traces_endpoint = traces_endpoint_from_env()?;
        let traces_protocol: Protocol = traces_protocol_from_env()?;
        Ok(Self {
            traces_endpoint,
            traces_protocol,
        })
    }

    /// Initializes the tracer, sets up the telemetry and subscriber layers, and sets the global subscriber.
    ///
    /// Note: this function should be called only once and be called by the binary entry point.
    pub fn init(&self) -> anyhow::Result<ShutdownGuard> {
        let tracer = self.init_tracer()?;
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

        let filter = EnvFilter::try_new("info,h2=off")?;

        let subscriber = Registry::default().with(telemetry).with(filter);

        tracing::subscriber::set_global_default(subscriber)?;
        Ok(ShutdownGuard)
    }

    fn init_tracer_http(&self) -> SpanExporterBuilder {
        opentelemetry_otlp::new_exporter()
            .http()
            .with_endpoint(&self.traces_endpoint)
            .into()
    }

    fn init_tracer_grpc(&self) -> SpanExporterBuilder {
        opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(&self.traces_endpoint)
            .into()
    }

    fn init_tracer(&self) -> Result<opentelemetry_sdk::trace::Tracer, TraceError> {
        let exporter = match self.traces_protocol {
            Protocol::HttpBinary => self.init_tracer_http(),
            Protocol::HttpJson => self.init_tracer_http(),
            Protocol::Grpc => self.init_tracer_grpc(),
        };

        opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(exporter)
            .with_trace_config(sdktrace::config())
            .install_batch(runtime::Tokio)
    }
}

/// Shutdown of the open telemetry services will automatically called when the OtelConfig instance goes out of scope.
#[must_use]
pub struct ShutdownGuard;

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        // Give tracer provider a chance to flush any pending traces.
        opentelemetry::global::shutdown_tracer_provider();
    }
}

/// Sets the OTLP endpoint from environment variables.
fn traces_endpoint_from_env() -> anyhow::Result<String> {
    Ok(env::var(OTEL_EXPORTER_OTLP_TRACES_ENDPOINT)
        .or_else(|_| env::var(OTEL_EXPORTER_OTLP_ENDPOINT))?)
}

/// Sets the OTLP protocol from environment variables.
fn traces_protocol_from_env() -> anyhow::Result<Protocol> {
    let traces_protocol = env::var(OTEL_EXPORTER_OTLP_TRACES_PROTOCOL).unwrap_or(
        env::var(OTEL_EXPORTER_OTLP_PROTOCOL)
            .unwrap_or(OTEL_EXPORTER_OTLP_PROTOCOL_DEFAULT.to_owned()),
    );
    let protocol = match traces_protocol.as_str() {
        OTEL_EXPORTER_OTLP_PROTOCOL_HTTP_PROTOBUF => Protocol::HttpBinary,
        OTEL_EXPORTER_OTLP_PROTOCOL_GRPC => Protocol::Grpc,
        _ => Err(TraceError::from(
            "Invalid OTEL_EXPORTER_OTLP_PROTOCOL value",
        ))?,
    };
    Ok(protocol)
}
