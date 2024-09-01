use data_encoding::BASE64;
use opentelemetry::{global, trace::TracerProvider, KeyValue};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{metrics::SdkMeterProvider, runtime, trace as sdktrace, Resource};
use opentelemetry_semantic_conventions::{
    resource::{DEPLOYMENT_ENVIRONMENT, SERVICE_NAME, SERVICE_VERSION},
    SCHEMA_URL,
};
use robotica_common::version::Version;
use serde::Deserialize;
use tap::Pipe;
use thiserror::Error;
use tonic::metadata::{errors::InvalidMetadataValue, MetadataMap};
use tracing_opentelemetry::{MetricsLayer, OpenTelemetryLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Deserialize)]
pub struct RemoteConfig {
    pub endpoint: String,
    pub username: String,
    pub password: String,
    pub organization: String,
    pub stream_name: String,
}

#[derive(Deserialize)]
pub struct Config {
    pub remote: Option<RemoteConfig>,
    pub deployment_environment: String,
}

// Create the required Metadata headers for OpenObserve
fn otlp_metadata(config: &RemoteConfig) -> Result<MetadataMap, InvalidMetadataValue> {
    let mut map = MetadataMap::with_capacity(3);
    let authorization_value =
        BASE64.encode(format!("{}:{}", config.username, config.password).as_bytes());
    map.insert(
        "authorization",
        format!("Basic {authorization_value}").parse()?,
    );
    map.insert("organization", config.organization.parse()?);
    map.insert("stream-name", config.stream_name.parse()?);

    Ok(map)
}

// Create a Resource that captures information about the entity for which telemetry is recorded.
fn resource(config: &Config) -> Resource {
    Resource::from_schema_url(
        [
            KeyValue::new(SERVICE_NAME, env!("CARGO_PKG_NAME")),
            KeyValue::new(SERVICE_VERSION, Version::get().vcs_ref),
            KeyValue::new(
                DEPLOYMENT_ENVIRONMENT,
                config.deployment_environment.clone(),
            ),
        ],
        SCHEMA_URL,
    )
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid metadata value: {0}")]
    InvalidMetadataValue(#[from] InvalidMetadataValue),

    #[error("Trace error: {0}")]
    Trace(#[from] opentelemetry::trace::TraceError),

    #[error("Metrics error: {0}")]
    Metrics(#[from] opentelemetry::metrics::MetricsError),

    #[error("Log error: {0}")]
    Log(#[from] opentelemetry::logs::LogError),

    #[error("TryInitError error: {0}")]
    TryInit(#[from] tracing_subscriber::util::TryInitError),
}

// Construct Tracer for OpenTelemetryLayer
fn init_tracer_provider(
    resource: &Resource,
    remote: &RemoteConfig,
) -> Result<sdktrace::TracerProvider, Error> {
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_tls_config(tonic::transport::ClientTlsConfig::new().with_enabled_roots())
                .with_endpoint(remote.endpoint.clone())
                // .with_interceptor(|request| {
                //     println!("xxxxx {request:?}");
                //     Ok(request)
                // })
                .with_metadata(otlp_metadata(remote)?),
        )
        .with_trace_config(
            opentelemetry_sdk::trace::Config::default().with_resource(resource.clone()),
        )
        .install_batch(runtime::Tokio)?
        .pipe(Ok)
}

fn init_metrics(
    resource: &Resource,
    remote: &RemoteConfig,
) -> Result<opentelemetry_sdk::metrics::SdkMeterProvider, Error> {
    let provider = opentelemetry_otlp::new_pipeline()
        .metrics(runtime::Tokio)
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_tls_config(tonic::transport::ClientTlsConfig::new().with_enabled_roots())
                .with_endpoint(remote.endpoint.clone())
                .with_metadata(otlp_metadata(remote)?),
        )
        .with_resource(resource.clone())
        .build();
    match provider {
        Ok(provider) => Ok(provider),
        Err(err) => Err(err.into()),
    }
}

fn init_logs(
    resource: &Resource,
    remote: &RemoteConfig,
) -> Result<opentelemetry_sdk::logs::LoggerProvider, Error> {
    opentelemetry_otlp::new_pipeline()
        .logging()
        .with_resource(resource.clone())
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_tls_config(tonic::transport::ClientTlsConfig::new().with_enabled_roots())
                .with_endpoint(remote.endpoint.clone())
                .with_metadata(otlp_metadata(remote)?),
        )
        .install_batch(runtime::Tokio)?
        .pipe(Ok)
}

// Initialize tracing-subscriber and return OtelGuard for opentelemetry-related termination processing
pub fn init_tracing_subscriber(config: &Config) -> Result<OtelGuard, Error> {
    // Add a tracing filter to filter events from crates used by opentelemetry-otlp.
    // The filter levels are set as follows:
    // - Allow `info` level and above by default.
    // - Restrict `hyper`, `tonic`, and `reqwest` to `error` level logs only.
    // This ensures events generated from these crates within the OTLP Exporter are not looped back,
    // thus preventing infinite event generation.
    // Note: This will also drop events from these crates used outside the OTLP Exporter.
    // For more details, see: https://github.com/open-telemetry/opentelemetry-rust/issues/761

    // FIXME - don't use unwrap!
    #[allow(clippy::unwrap_used)]
    let filter = EnvFilter::new("info")
        .add_directive("hyper=error".parse().unwrap())
        .add_directive("tonic=error".parse().unwrap())
        .add_directive("reqwest=error".parse().unwrap());

    let layer = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer());

    if let Some(remote) = &config.remote {
        let resource = resource(config);
        let meter_provider = init_metrics(&resource, remote)?;
        let logger_provider = init_logs(&resource, remote)?;
        let tracer_provider = init_tracer_provider(&resource, remote)?;

        global::set_tracer_provider(tracer_provider.clone());
        global::set_meter_provider(meter_provider.clone());

        let tracer = tracer_provider.tracer_builder("brian-backend").build();

        layer
            .with(MetricsLayer::new(meter_provider.clone()))
            .with(OpenTelemetryLayer::new(tracer))
            .with(OpenTelemetryTracingBridge::new(&logger_provider))
            .try_init()?;

        Ok(OtelGuard {
            meter_provider: Some(meter_provider),
            logger_provider: Some(logger_provider),
        })
    } else {
        layer.init();

        Ok(OtelGuard {
            meter_provider: None,
            logger_provider: None,
        })
    }
}

pub struct OtelGuard {
    meter_provider: Option<SdkMeterProvider>,
    logger_provider: Option<opentelemetry_sdk::logs::LoggerProvider>,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        global::shutdown_tracer_provider();

        if let Some(provider) = self.meter_provider.take() {
            if let Err(err) = provider.shutdown() {
                eprintln!("{err:?}");
            }
        }
        if let Some(provider) = self.logger_provider.take() {
            if let Err(err) = provider.shutdown() {
                eprintln!("{err:?}");
            }
        }

        opentelemetry::global::shutdown_tracer_provider();
    }
}
