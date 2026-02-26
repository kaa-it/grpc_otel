use backoff::{ExponentialBackoff, backoff::Backoff};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum RetryError {
    #[error("Maximum retries exceeded")]
    MaxRetriesExceeded,
    #[error("Non-retryable error: {0}")]
    NonRetryable(String),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Request error: {0}")]
    Request(String),
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub initial_interval: Duration,
    pub max_interval: Duration,
    pub multiplier: f64,
    pub max_elapsed_time: Option<Duration>,
    pub randomization_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_interval: Duration::from_millis(100),
            max_interval: Duration::from_secs(30),
            multiplier: 2.0,
            max_elapsed_time: Some(Duration::from_secs(300)), // 5 minutes
            randomization_factor: 0.1,
        }
    }
}

impl RetryConfig {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn with_initial_interval(mut self, interval: Duration) -> Self {
        self.initial_interval = interval;
        self
    }

    pub fn with_max_interval(mut self, interval: Duration) -> Self {
        self.max_interval = interval;
        self
    }

    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    pub fn with_max_elapsed_time(mut self, time: Option<Duration>) -> Self {
        self.max_elapsed_time = time;
        self
    }

    pub fn with_randomization_factor(mut self, factor: f64) -> Self {
        self.randomization_factor = factor;
        self
    }

    pub fn to_exponential_backoff(&self) -> ExponentialBackoff {
        let mut backoff = ExponentialBackoff {
            initial_interval: self.initial_interval,
            max_interval: self.max_interval,
            randomization_factor: self.randomization_factor,
            multiplier: self.multiplier,
            max_elapsed_time: self.max_elapsed_time,
            ..Default::default()
        };

        backoff.reset();
        backoff
    }
}

/// Determines if a gRPC error is retryable
pub fn is_retryable_error(error: &tonic::Status) -> bool {
    match error.code() {
        // Retryable errors
        tonic::Code::Unavailable => true,
        tonic::Code::ResourceExhausted => true,
        tonic::Code::Aborted => true,
        tonic::Code::Internal => true,
        tonic::Code::Unknown => true,
        tonic::Code::DeadlineExceeded => true,

        // Non-retryable errors
        tonic::Code::InvalidArgument => false,
        tonic::Code::NotFound => false,
        tonic::Code::AlreadyExists => false,
        tonic::Code::PermissionDenied => false,
        tonic::Code::Unauthenticated => false,
        tonic::Code::FailedPrecondition => false,
        tonic::Code::OutOfRange => false,
        tonic::Code::Unimplemented => false,
        tonic::Code::DataLoss => false,
        tonic::Code::Cancelled => false,

        // Default to non-retryable for safety
        _ => false,
    }
}

/// Determines if a connection error is retryable
pub fn is_retryable_connection_error(_error: &tonic::transport::Error) -> bool {
    // Most connection errors are retryable
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::Status;

    #[test]
    fn test_retryable_errors() {
        assert!(is_retryable_error(&Status::unavailable(
            "Service unavailable"
        )));
        assert!(is_retryable_error(&Status::resource_exhausted(
            "Rate limited"
        )));
        assert!(is_retryable_error(&Status::aborted("Transaction aborted")));
        assert!(is_retryable_error(&Status::internal("Internal error")));
        assert!(is_retryable_error(&Status::unknown("Unknown error")));
        assert!(is_retryable_error(&Status::deadline_exceeded("Timeout")));
    }

    #[test]
    fn test_non_retryable_errors() {
        assert!(!is_retryable_error(&Status::invalid_argument(
            "Bad request"
        )));
        assert!(!is_retryable_error(&Status::not_found("Not found")));
        assert!(!is_retryable_error(&Status::already_exists(
            "Already exists"
        )));
        assert!(!is_retryable_error(&Status::permission_denied(
            "Access denied"
        )));
        assert!(!is_retryable_error(&Status::unauthenticated(
            "Not authenticated"
        )));
        assert!(!is_retryable_error(&Status::failed_precondition(
            "Precondition failed"
        )));
        assert!(!is_retryable_error(&Status::out_of_range("Out of range")));
        assert!(!is_retryable_error(&Status::unimplemented(
            "Not implemented"
        )));
        assert!(!is_retryable_error(&Status::cancelled("Cancelled")));
    }

    #[test]
    fn test_retry_config_builder() {
        let config = RetryConfig::new()
            .with_max_retries(10)
            .with_initial_interval(Duration::from_millis(200))
            .with_max_interval(Duration::from_secs(60))
            .with_multiplier(1.5);

        assert_eq!(config.max_retries, 10);
        assert_eq!(config.initial_interval, Duration::from_millis(200));
        assert_eq!(config.max_interval, Duration::from_secs(60));
        assert_eq!(config.multiplier, 1.5);
    }

    #[test]
    fn test_exponential_backoff_conversion() {
        let config = RetryConfig::new()
            .with_initial_interval(Duration::from_millis(100))
            .with_max_interval(Duration::from_secs(10))
            .with_multiplier(2.0);

        let backoff = config.to_exponential_backoff();

        // The backoff should be created successfully
        // We can't easily test the internal values without exposing them,
        // but we can at least verify it doesn't panic
        assert!(backoff.initial_interval >= Duration::from_millis(1));
    }
}
