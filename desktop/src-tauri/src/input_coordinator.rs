use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// A pending input waiting to be processed
#[derive(Debug, Clone)]
pub struct PendingInput {
    pub session_id: String,
    pub text: String,
    pub sender_id: String,
    pub timestamp: Instant,
}

/// Coordinates input from multiple clients to prevent race conditions.
///
/// When multiple devices (mobile, desktop client) send input simultaneously,
/// this coordinator ensures orderly processing by:
/// 1. Tracking who sent the last input
/// 2. Implementing debounce between different senders
/// 3. Queueing inputs that come too quickly
pub struct InputCoordinator {
    queue: Mutex<VecDeque<PendingInput>>,
    last_input_by: Mutex<Option<(String, Instant)>>, // (sender_id, time)
    debounce_ms: u64,
}

impl InputCoordinator {
    /// Create a new InputCoordinator with the specified debounce time in milliseconds.
    ///
    /// The debounce time determines how long to wait between inputs from different senders.
    /// Inputs from the same sender are always allowed immediately.
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            last_input_by: Mutex::new(None),
            debounce_ms,
        }
    }

    /// Submit an input for processing.
    ///
    /// Returns `Ok(true)` if the input can be executed immediately.
    /// Returns `Ok(false)` if the input was queued due to debounce.
    ///
    /// # Arguments
    ///
    /// * `input` - The pending input to submit
    pub async fn submit_input(&self, input: PendingInput) -> Result<bool, String> {
        let mut last = self.last_input_by.lock().await;

        if let Some((ref last_sender, last_time)) = *last {
            // Different sender? Check debounce
            if last_sender != &input.sender_id {
                let elapsed = last_time.elapsed();
                if elapsed < Duration::from_millis(self.debounce_ms) {
                    // Queue it instead of immediate execution
                    tracing::debug!(
                        "Input from {} queued (last input from {} was {}ms ago)",
                        input.sender_id,
                        last_sender,
                        elapsed.as_millis()
                    );
                    self.queue.lock().await.push_back(input);
                    return Ok(false); // Queued, not executed
                }
            }
        }

        // Execute immediately
        *last = Some((input.sender_id.clone(), Instant::now()));
        Ok(true) // Can execute now
    }

    /// Process any queued inputs that have waited long enough.
    ///
    /// Returns a vector of inputs ready to be processed.
    pub async fn process_queue(&self) -> Vec<PendingInput> {
        let mut queue = self.queue.lock().await;
        let now = Instant::now();

        let ready: Vec<_> = queue
            .iter()
            .filter(|i| now.duration_since(i.timestamp) >= Duration::from_millis(self.debounce_ms))
            .cloned()
            .collect();

        // Remove processed items from queue
        for _ in 0..ready.len() {
            queue.pop_front();
        }

        ready
    }

    /// Get the current queue length.
    pub async fn queue_length(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// Check if a specific sender can send input immediately.
    ///
    /// Returns `true` if the sender can send immediately (same sender as last or debounce passed).
    pub async fn can_send_immediately(&self, sender_id: &str) -> bool {
        let last = self.last_input_by.lock().await;

        match *last {
            None => true,
            Some((ref last_sender, last_time)) => {
                last_sender == sender_id
                    || last_time.elapsed() >= Duration::from_millis(self.debounce_ms)
            }
        }
    }

    /// Get the ID of the sender who last sent input, if any.
    pub async fn last_sender(&self) -> Option<String> {
        self.last_input_by
            .lock()
            .await
            .as_ref()
            .map(|(id, _)| id.clone())
    }

    /// Clear the queue and reset the last sender tracking.
    ///
    /// Useful when a session is closed or reset.
    pub async fn reset(&self) {
        *self.last_input_by.lock().await = None;
        self.queue.lock().await.clear();
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_same_sender_immediate() {
        let coordinator = InputCoordinator::new(500);

        let input1 = PendingInput {
            session_id: "session-1".to_string(),
            text: "hello".to_string(),
            sender_id: "mobile-1".to_string(),
            timestamp: Instant::now(),
        };

        let input2 = PendingInput {
            session_id: "session-1".to_string(),
            text: "world".to_string(),
            sender_id: "mobile-1".to_string(),
            timestamp: Instant::now(),
        };

        // Both from same sender should execute immediately
        assert!(coordinator.submit_input(input1).await.unwrap());
        assert!(coordinator.submit_input(input2).await.unwrap());
    }

    #[tokio::test]
    async fn test_different_sender_debounce() {
        let coordinator = InputCoordinator::new(100); // 100ms debounce for faster test

        let input1 = PendingInput {
            session_id: "session-1".to_string(),
            text: "hello".to_string(),
            sender_id: "mobile-1".to_string(),
            timestamp: Instant::now(),
        };

        let input2 = PendingInput {
            session_id: "session-1".to_string(),
            text: "world".to_string(),
            sender_id: "desktop-1".to_string(),
            timestamp: Instant::now(),
        };

        // First input executes immediately
        assert!(coordinator.submit_input(input1).await.unwrap());

        // Second input from different sender should be queued
        assert!(!coordinator.submit_input(input2).await.unwrap());

        // Queue should have 1 item
        assert_eq!(coordinator.queue_length().await, 1);
    }

    #[tokio::test]
    async fn test_debounce_expires() {
        let coordinator = InputCoordinator::new(50); // 50ms debounce

        let input1 = PendingInput {
            session_id: "session-1".to_string(),
            text: "hello".to_string(),
            sender_id: "mobile-1".to_string(),
            timestamp: Instant::now(),
        };

        assert!(coordinator.submit_input(input1).await.unwrap());

        // Wait for debounce to expire
        sleep(Duration::from_millis(60)).await;

        let input2 = PendingInput {
            session_id: "session-1".to_string(),
            text: "world".to_string(),
            sender_id: "desktop-1".to_string(),
            timestamp: Instant::now(),
        };

        // After debounce, different sender should execute immediately
        assert!(coordinator.submit_input(input2).await.unwrap());
    }

    #[tokio::test]
    async fn test_reset() {
        let coordinator = InputCoordinator::new(500);

        let input = PendingInput {
            session_id: "session-1".to_string(),
            text: "hello".to_string(),
            sender_id: "mobile-1".to_string(),
            timestamp: Instant::now(),
        };

        coordinator.submit_input(input).await.unwrap();
        assert!(coordinator.last_sender().await.is_some());

        coordinator.reset().await;

        assert!(coordinator.last_sender().await.is_none());
        assert_eq!(coordinator.queue_length().await, 0);
    }
}
