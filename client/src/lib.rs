use device_command::{
    ConnectCommand, SendCommandRequest, command_service_client::CommandServiceClient,
    send_command_request,
};
use tonic::transport::Channel;
use tonic_tracing_opentelemetry::middleware::client::{OtelGrpcLayer, OtelGrpcService};
use tower::ServiceBuilder;

pub async fn run() -> anyhow::Result<()> {
    let _guard = init_tracing_opentelemetry::TracingConfig::development().init_subscriber()?;
    // telemetry::init_subscriber(
    //     "client".to_string(),
    //     "info".to_string(),
    //     Some("http://localhost:4317".to_string()),
    //     std::io::stdout,
    // );

    let channel = Channel::from_shared("http://localhost:50051".to_string())?
        .connect()
        .await?;

    let channel = ServiceBuilder::new()
        .layer(OtelGrpcLayer::default())
        .service(channel);

    let mut client = CommandServiceClient::new(channel);

    send_command(&mut client).await?;

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    Ok(())
}

#[tracing::instrument(skip(client))]
async fn send_command(
    client: &mut CommandServiceClient<OtelGrpcService<Channel>>,
) -> anyhow::Result<()> {
    let request = SendCommandRequest {
        command: Some(send_command_request::Command::Connect(ConnectCommand {})),
    };

    let response = client.send_command(request).await?;

    println!("Response: {:?}", response);

    Ok(())
}
