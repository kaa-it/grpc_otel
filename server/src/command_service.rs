use device_command::{ConnectedResponse, DisconnectedResponse, SendCommandRequest, SendCommandResponse, command_service_server::CommandService, send_command_request::Command, send_command_response};
use tonic::{Request, Response, Status};

pub struct CommandServiceImpl;

impl CommandServiceImpl {
    pub fn new() -> Self {
        Self {}
    }

    #[tracing::instrument(skip(self))]
    async fn fake(&self) {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

#[tonic::async_trait]
impl CommandService for CommandServiceImpl {
    async fn send_command(
        &self,
        request: Request<SendCommandRequest>,
    ) -> Result<Response<SendCommandResponse>, Status> {
        
        let request = request.into_inner();

        tracing::info!("Received command: {:?}", request.command);

        self.fake().await;

        match request.command.unwrap() {
            Command::Connect(_) => {
                return Ok(Response::new(SendCommandResponse {
                    response: Some(send_command_response::Response::Connected(ConnectedResponse {})),
                        
                }));
            },
            Command::Disconnect(_) => {
                return Ok(Response::new(SendCommandResponse {
                    response: Some(send_command_response::Response::Disconnected(DisconnectedResponse {})),
                }));
            },
        }
    }
}
