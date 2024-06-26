use data_encoding::BASE64;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    metrics::{
        reader::{DefaultAggregationSelector, DefaultTemporalitySelector},
        MeterProviderBuilder, PeriodicReader, SdkMeterProvider,
    },
    runtime,
    trace::{BatchConfig, RandomIdGenerator, Sampler, Tracer},
    Resource,
};
use opentelemetry_semantic_conventions::{
    resource::{DEPLOYMENT_ENVIRONMENT, SERVICE_NAME, SERVICE_VERSION},
    SCHEMA_URL,
};
use robotica_common::version::Version;
use serde::Deserialize;
use tap::Pipe;
use thiserror::Error;
use tonic::metadata::{errors::InvalidMetadataValue, MetadataMap};
use tracing_core::Level;
use tracing_opentelemetry::{MetricsLayer, OpenTelemetryLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
}

// Construct MeterProvider for MetricsLayer
fn init_meter_provider(config: &Config) -> Result<SdkMeterProvider, Error> {
    let reader = if let Some(remote) = &config.remote {
        let exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(remote.endpoint.clone())
            .with_metadata(otlp_metadata(remote)?)
            .build_metrics_exporter(
                Box::new(DefaultAggregationSelector::new()),
                Box::new(DefaultTemporalitySelector::new()),
            )?;

        PeriodicReader::builder(exporter, runtime::Tokio)
            .with_interval(std::time::Duration::from_secs(30))
            .build()
            .pipe(Some)
    } else {
        None
    };

    // For debugging in development
    let stdout_reader = PeriodicReader::builder(
        opentelemetry_stdout::MetricsExporter::default(),
        runtime::Tokio,
    )
    .build();

    let meter_provider = MeterProviderBuilder::default()
        .with_resource(resource(config))
        .pipe(|p| {
            if let Some(reader) = reader {
                p.with_reader(reader)
            } else {
                p
            }
        })
        .with_reader(stdout_reader)
        .build();

    global::set_meter_provider(meter_provider.clone());

    Ok(meter_provider)
}

// Construct Tracer for OpenTelemetryLayer
fn init_tracer(config: &Config, remote: &RemoteConfig) -> Result<Tracer, Error> {
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_trace_config(
            opentelemetry_sdk::trace::Config::default()
                // Customize sampling strategy
                .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
                    1.0,
                ))))
                // If export trace to AWS X-Ray, you can use XrayIdGenerator
                .with_id_generator(RandomIdGenerator::default())
                .with_resource(resource(config)),
        )
        .with_batch_config(BatchConfig::default())
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(remote.endpoint.clone())
                .with_metadata(otlp_metadata(remote)?),
        )
        .install_batch(runtime::Tokio)?
        .pipe(Ok)
}

// Initialize tracing-subscriber and return OtelGuard for opentelemetry-related termination processing
pub fn init_tracing_subscriber(config: &Config) -> Result<OtelGuard, Error> {
    let meter_provider = init_meter_provider(config)?;

    let layer = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::from_level(
            Level::INFO,
        ))
        .with(tracing_subscriber::fmt::layer())
        .with(MetricsLayer::new(meter_provider.clone()));

    if let Some(remote) = &config.remote {
        layer
            .with(OpenTelemetryLayer::new(init_tracer(config, remote)?))
            .init();
    } else {
        layer.init();
    }

    Ok(OtelGuard { meter_provider })
}

pub struct OtelGuard {
    meter_provider: SdkMeterProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(err) = self.meter_provider.shutdown() {
            eprintln!("{err:?}");
        }
        opentelemetry::global::shutdown_tracer_provider();
    }
}
