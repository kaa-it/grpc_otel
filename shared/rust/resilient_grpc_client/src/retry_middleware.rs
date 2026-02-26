use crate::retry_policy::{RetryConfig, RetryError, is_retryable_error};
use backoff::backoff::Backoff;
use futures::future::BoxFuture;
use std::future::Future;
use std::task::{Context, Poll};
use tower::{Layer, Service};
use tracing::{debug, warn};

/// A Tower layer that adds retry functionality to gRPC services
#[derive(Clone, Debug)]
pub struct RetryLayer {
    config: RetryConfig,
}

impl RetryLayer {
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }
}

impl<S> Layer<S> for RetryLayer {
    type Service = RetryService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RetryService {
            inner,
            config: self.config.clone(),
        }
    }
}

/// A Tower service that implements retry logic for gRPC requests
#[derive(Clone, Debug)]
pub struct RetryService<S> {
    inner: S,
    config: RetryConfig,
}

impl<S> RetryService<S> {
    pub fn new(inner: S, config: RetryConfig) -> Self {
        Self { inner, config }
    }
}

impl<S, Request> Service<Request> for RetryService<S>
where
    S: Service<Request> + Clone + Send + 'static,
    S::Response: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    S::Future: Send + 'static,
    Request: Clone + Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let mut service = self.inner.clone();
        let config = self.config.clone();

        Box::pin(async move {
            let mut backoff = config.to_exponential_backoff();
            let mut attempt = 0;

            loop {
                attempt += 1;
                debug!("Attempt {} for gRPC request", attempt);

                let result = service.call(request.clone()).await;

                match result {
                    Ok(response) => {
                        debug!("gRPC request succeeded on attempt {}", attempt);
                        return Ok(response);
                    }
                    Err(error) => {
                        // Convert error to check if it's a gRPC status error
                        let should_retry =
                            if let Ok(status) = error.into().downcast::<tonic::Status>() {
                                is_retryable_error(&status)
                            } else {
                                // For non-gRPC errors, we generally retry
                                true
                            };

                        if !should_retry {
                            warn!("Non-retryable error encountered on attempt {}", attempt);
                            // We need to reconstruct the error since we consumed it
                            let result = service.call(request.clone()).await;
                            return result;
                        }

                        if attempt >= config.max_retries {
                            warn!("Maximum retries ({}) exceeded", config.max_retries);
                            let result = service.call(request.clone()).await;
                            return result;
                        }

                        if let Some(delay) = backoff.next_backoff() {
                            debug!("Retrying in {:?} (attempt {})", delay, attempt);
                            tokio::time::sleep(delay).await;
                        } else {
                            warn!("Backoff timer exhausted");
                            let result = service.call(request.clone()).await;
                            return result;
                        }
                    }
                }
            }
        })
    }
}

/// Retry function for standalone use outside of Tower middleware
pub async fn retry_with_backoff<F, Fut, T, E>(
    mut operation: F,
    config: &RetryConfig,
) -> Result<T, RetryError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut backoff = config.to_exponential_backoff();
    let mut attempt = 0;

    loop {
        attempt += 1;
        debug!("Executing operation, attempt {}", attempt);

        match operation().await {
            Ok(result) => {
                debug!("Operation succeeded on attempt {}", attempt);
                return Ok(result);
            }
            Err(error) => {
                warn!("Operation failed on attempt {}: {}", attempt, error);

                if attempt >= config.max_retries {
                    warn!("Maximum retries ({}) exceeded", config.max_retries);
                    return Err(RetryError::MaxRetriesExceeded);
                }

                if let Some(delay) = backoff.next_backoff() {
                    debug!("Retrying in {:?} (attempt {})", delay, attempt);
                    tokio::time::sleep(delay).await;
                } else {
                    warn!("Backoff timer exhausted");
                    return Err(RetryError::MaxRetriesExceeded);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tower::ServiceExt;

    #[derive(Clone)]
    struct MockService {
        call_count: Arc<Mutex<usize>>,
        fail_until: usize,
    }

    impl MockService {
        fn new(fail_until: usize) -> Self {
            Self {
                call_count: Arc::new(Mutex::new(0)),
                fail_until,
            }
        }

        fn call_count(&self) -> usize {
            *self.call_count.lock().unwrap()
        }
    }

    impl Service<String> for MockService {
        type Response = String;
        type Error = tonic::Status;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: String) -> Self::Future {
            let call_count = self.call_count.clone();
            let fail_until = self.fail_until;

            Box::pin(async move {
                let mut count = call_count.lock().unwrap();
                *count += 1;
                let current_count = *count;
                drop(count);

                if current_count <= fail_until {
                    Err(tonic::Status::unavailable("Service unavailable"))
                } else {
                    Ok(format!("Response to: {}", request))
                }
            })
        }
    }

    #[tokio::test]
    async fn test_retry_service_success_after_failures() {
        let mock_service = MockService::new(2); // Fail first 2 attempts
        let config = RetryConfig::new()
            .with_max_retries(5)
            .with_initial_interval(Duration::from_millis(10));

        let retry_service = RetryService::new(mock_service.clone(), config);

        let result = retry_service
            .oneshot("test request".to_string())
            .await
            .unwrap();

        assert_eq!(result, "Response to: test request");
        assert_eq!(mock_service.call_count(), 3); // 2 failures + 1 success
    }

    #[tokio::test]
    async fn test_retry_service_max_retries_exceeded() {
        let mock_service = MockService::new(10); // Always fail
        let config = RetryConfig::new()
            .with_max_retries(3)
            .with_initial_interval(Duration::from_millis(10));

        let retry_service = RetryService::new(mock_service.clone(), config);

        let result = retry_service.oneshot("test request".to_string()).await;

        assert!(result.is_err());
        assert_eq!(mock_service.call_count(), 4); // 3 retries + 1 final attempt
    }

    #[tokio::test]
    async fn test_retry_with_backoff_function() {
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let config = RetryConfig::new()
            .with_max_retries(3)
            .with_initial_interval(Duration::from_millis(10));

        let result = retry_with_backoff(
            || {
                let count = call_count_clone.clone();
                async move {
                    let mut c = count.lock().unwrap();
                    *c += 1;
                    let current = *c;
                    drop(c);

                    if current <= 2 {
                        Err("Temporary failure")
                    } else {
                        Ok("Success")
                    }
                }
            },
            &config,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Success");
        assert_eq!(*call_count.lock().unwrap(), 3);
    }
}
