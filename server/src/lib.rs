use device_command::command_service_server::CommandServiceServer;
use tokio::sync::watch::{self, Receiver};
use tonic::transport::Server;
use tonic_tracing_opentelemetry::middleware::{filters, server};

mod command_service;

pub async fn run() -> anyhow::Result<()> {
    // telemetry::init_subscriber(
    //     "server".to_string(),
    //     "info".to_string(),
    //     Some("http://localhost:4317".to_string()),
    //     std::io::stdout,
    // );

    let _guard = init_tracing_opentelemetry::TracingConfig::debug().init_subscriber()?;

    let (sender, receiver) = watch::channel("work");

    signals::signals_register(&sender).await?;

    let stop = receiver.clone();

    _ = run_command_service(stop).await?;

    signals::signals_unregister().await?;

    tracing::info!("Server terminated");

    Ok(())
}

async fn run_command_service(
    mut stop: Receiver<&'static str>,
) -> anyhow::Result<()> {

    let reflection_service = tonic_reflection::server::Builder::configure()
    .register_encoded_file_descriptor_set(device_command::reflection::FILE_DESCRIPTOR_SET)
    .build_v1()?;

    let command_service = command_service::CommandServiceImpl::new();

    let mut f = async || stop.changed().await.unwrap_or(());

    Server::builder()
        .layer(server::OtelGrpcLayer::default().filter(filters::reject_healthcheck))
        .add_service(reflection_service)
        .add_service(CommandServiceServer::new(command_service))
        .serve_with_shutdown("0.0.0.0:50051".parse()?, f())
        .await?;

    Ok(())
}