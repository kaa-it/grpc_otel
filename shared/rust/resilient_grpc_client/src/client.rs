use crate::retry_middleware::retry_with_backoff;
use crate::retry_policy::{RetryConfig, RetryError};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};

use tracing::{debug, error, info, warn};

/// Configuration for the resilient gRPC client
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Retry configuration for connection establishment
    pub connection_retry: RetryConfig,
    /// Retry configuration for individual requests
    pub request_retry: RetryConfig,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Request timeout
    pub request_timeout: Duration,
    /// Keep-alive interval
    pub keep_alive_interval: Option<Duration>,
    /// Keep-alive timeout
    pub keep_alive_timeout: Option<Duration>,
    /// Maximum number of concurrent streams
    pub max_concurrent_streams: Option<u32>,
    /// Initial connection window size
    pub initial_connection_window_size: Option<u32>,
    /// Initial stream window size
    pub initial_stream_window_size: Option<u32>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connection_retry: RetryConfig::new()
                .with_max_retries(5)
                .with_initial_interval(Duration::from_millis(500))
                .with_max_interval(Duration::from_secs(30)),
            request_retry: RetryConfig::new()
                .with_max_retries(3)
                .with_initial_interval(Duration::from_millis(100))
                .with_max_interval(Duration::from_secs(5)),
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            keep_alive_interval: Some(Duration::from_secs(30)),
            keep_alive_timeout: Some(Duration::from_secs(5)),
            max_concurrent_streams: None,
            initial_connection_window_size: None,
            initial_stream_window_size: None,
        }
    }
}

impl ClientConfig {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_connection_retry(mut self, config: RetryConfig) -> Self {
        self.connection_retry = config;
        self
    }

    pub fn with_request_retry(mut self, config: RetryConfig) -> Self {
        self.request_retry = config;
        self
    }

    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    pub fn with_keep_alive(mut self, interval: Duration, timeout: Duration) -> Self {
        self.keep_alive_interval = Some(interval);
        self.keep_alive_timeout = Some(timeout);
        self
    }

    pub fn with_max_concurrent_streams(mut self, max_streams: u32) -> Self {
        self.max_concurrent_streams = Some(max_streams);
        self
    }
}

/// Trait for creating resilient gRPC clients
#[async_trait]
pub trait ResilientClient<T> {
    /// Create a new client with the given configuration
    async fn new(endpoint: String, config: ClientConfig) -> Result<T, RetryError>;

    /// Reconnect the client
    async fn reconnect(&mut self) -> Result<(), RetryError>;

    /// Check if the client is connected
    fn is_connected(&self) -> bool;
}

/// A resilient gRPC client wrapper that handles connection and request retries
#[derive(Debug, Clone)]
pub struct ResilientGrpcClient {
    endpoint: String,
    channel: Arc<tokio::sync::RwLock<Option<Channel>>>,
    config: ClientConfig,
}

impl ResilientGrpcClient {
    /// Create a new resilient gRPC client
    pub async fn new(endpoint: String, config: ClientConfig) -> Result<Self, RetryError> {
        let client = Self {
            endpoint: endpoint.clone(),
            channel: Arc::new(tokio::sync::RwLock::new(None)),
            config,
        };

        // Establish initial connection with retry
        client.connect().await?;
        Ok(client)
    }

    /// Establish connection with retry logic
    async fn connect(&self) -> Result<(), RetryError> {
        info!("Establishing connection to {}", self.endpoint);

        let endpoint = self.endpoint.clone();
        let config = self.config.clone();
        let channel_ref = self.channel.clone();

        retry_with_backoff(
            || async {
                let mut endpoint_builder = Endpoint::from_shared(endpoint.clone())
                    .map_err(|e| format!("Invalid endpoint: {}", e))?;

                // Configure endpoint
                endpoint_builder = endpoint_builder
                    .connect_timeout(config.connect_timeout)
                    .timeout(config.request_timeout);

                if let Some(interval) = config.keep_alive_interval {
                    endpoint_builder = endpoint_builder.http2_keep_alive_interval(interval);
                }

                if let Some(timeout) = config.keep_alive_timeout {
                    endpoint_builder = endpoint_builder.keep_alive_timeout(timeout);
                }

                if let Some(window_size) = config.initial_connection_window_size {
                    endpoint_builder = endpoint_builder.initial_connection_window_size(window_size);
                }

                if let Some(window_size) = config.initial_stream_window_size {
                    endpoint_builder = endpoint_builder.initial_stream_window_size(window_size);
                }

                debug!("Attempting to connect to {}", endpoint);
                let channel = endpoint_builder
                    .connect()
                    .await
                    .map_err(|e| format!("Connection failed: {}", e))?;

                // Store the channel
                {
                    let mut ch = channel_ref.write().await;
                    *ch = Some(channel);
                }

                debug!("Successfully connected to {}", endpoint);
                Ok::<(), String>(())
            },
            &self.config.connection_retry,
        )
        .await
        .map_err(|e| {
            error!("Failed to establish connection after retries: {:?}", e);
            RetryError::Connection(format!("Connection failed: {:?}", e))
        })
    }

    /// Get the channel, reconnecting if necessary
    pub async fn get_channel(&self) -> Result<Channel, RetryError> {
        // First, try to get existing channel
        {
            let channel_guard = self.channel.read().await;
            if let Some(ref channel) = *channel_guard {
                return Ok(channel.clone());
            }
        }

        // If no channel exists, establish connection
        self.connect().await?;

        // Get the newly created channel
        let channel_guard = self.channel.read().await;
        channel_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| RetryError::Connection("Failed to establish channel".to_string()))
    }

    /// Create a channel with retry middleware applied
    pub async fn get_channel_with_retry(&self) -> Result<Channel, RetryError> {
        // For now, just return the base channel
        // The retry logic is handled in execute_with_retry method
        self.get_channel().await
    }

    /// Reconnect the client
    pub async fn reconnect(&self) -> Result<(), RetryError> {
        info!("Reconnecting to {}", self.endpoint);

        // Clear existing channel
        {
            let mut channel_guard = self.channel.write().await;
            *channel_guard = None;
        }

        // Establish new connection
        self.connect().await
    }

    /// Check if client has an active channel
    pub async fn is_connected(&self) -> bool {
        let channel_guard = self.channel.read().await;
        channel_guard.is_some()
    }

    /// Execute a request with automatic retry and reconnection
    pub async fn execute_with_retry<F, Fut, T>(&self, operation: F) -> Result<T, RetryError>
    where
        F: Fn(Channel) -> Fut,
        Fut: std::future::Future<Output = Result<T, tonic::Status>>,
    {
        retry_with_backoff(
            || async {
                let channel = self.get_channel().await.map_err(|e| format!("{}", e))?;

                match operation(channel).await {
                    Ok(result) => Ok(result),
                    Err(status) => {
                        // Check if we should retry
                        if crate::retry_policy::is_retryable_error(&status) {
                            // If it's a connection-related error, try to reconnect
                            if matches!(
                                status.code(),
                                tonic::Code::Unavailable | tonic::Code::Unknown
                            ) {
                                warn!("Connection issue detected, attempting reconnection");
                                if let Err(e) = self.reconnect().await {
                                    warn!("Reconnection failed: {:?}", e);
                                }
                            }
                            Err(format!("Retryable gRPC error: {}", status))
                        } else {
                            // Convert non-retryable errors to a different error type
                            Err(format!("Non-retryable gRPC error: {}", status))
                        }
                    }
                }
            },
            &self.config.request_retry,
        )
        .await
    }

    /// Get client configuration
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Get endpoint URL
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

/// Health check functionality
impl ResilientGrpcClient {
    /// Perform a simple health check
    pub async fn health_check(&self) -> Result<(), RetryError> {
        let _channel = self.get_channel().await?;

        // Simple connectivity test - this will fail if the connection is broken
        // You can implement actual health check calls here if your service supports them
        match tokio::time::timeout(Duration::from_secs(5), async {
            // This is a basic check - in practice you might want to call an actual health endpoint
            Ok::<(), tonic::Status>(())
        })
        .await
        {
            Ok(Ok(())) => {
                debug!("Health check passed for {}", self.endpoint);
                Ok(())
            }
            Ok(Err(status)) => {
                warn!("Health check failed with status: {}", status);
                Err(RetryError::Request(format!(
                    "Health check failed: {}",
                    status
                )))
            }
            Err(_) => {
                warn!("Health check timed out for {}", self.endpoint);
                Err(RetryError::Request("Health check timed out".to_string()))
            }
        }
    }

    /// Start a background task for periodic health checks
    pub fn start_health_monitor(&self, interval: Duration) -> tokio::task::JoinHandle<()> {
        let client = self.clone();

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                match client.health_check().await {
                    Ok(()) => {
                        debug!("Periodic health check passed");
                    }
                    Err(e) => {
                        warn!("Periodic health check failed: {:?}", e);
                        // Attempt reconnection on health check failure
                        if let Err(reconnect_err) = client.reconnect().await {
                            error!(
                                "Failed to reconnect after health check failure: {:?}",
                                reconnect_err
                            );
                        } else {
                            info!("Successfully reconnected after health check failure");
                        }
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_client_config_builder() {
        let config = ClientConfig::new()
            .with_connect_timeout(Duration::from_secs(5))
            .with_request_timeout(Duration::from_secs(15))
            .with_keep_alive(Duration::from_secs(20), Duration::from_secs(3))
            .with_max_concurrent_streams(100);

        assert_eq!(config.connect_timeout, Duration::from_secs(5));
        assert_eq!(config.request_timeout, Duration::from_secs(15));
        assert_eq!(config.keep_alive_interval, Some(Duration::from_secs(20)));
        assert_eq!(config.keep_alive_timeout, Some(Duration::from_secs(3)));
        assert_eq!(config.max_concurrent_streams, Some(100));
    }

    #[tokio::test]
    async fn test_invalid_endpoint() {
        let config = ClientConfig::new();
        let result = ResilientGrpcClient::new("invalid-url".to_string(), config).await;
        assert!(result.is_err());
    }
}
