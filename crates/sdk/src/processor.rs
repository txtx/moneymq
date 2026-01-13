use std::sync::Arc;

use futures::StreamExt;
use parking_lot::RwLock;
use reqwest_eventsource::{Event as SseEvent, EventSource};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::{
    actor::PaymentHook,
    error::{PaymentStreamError, Result},
    types::{BasketItem, ChannelConfig, ConnectionState, PaymentDetails, Transaction, defaults},
};

/// Transaction context - wraps a transaction with its scoped hook
///
/// When a transaction is received, the processor creates a TransactionContext
/// that contains the transaction data and provides a factory method to create
/// a PaymentHook scoped to that transaction's channel.
pub struct TransactionContext {
    /// The transaction data
    pub transaction: Transaction,

    /// Configuration for creating hooks
    config: ChannelConfig,

    /// Pre-connected hook (optional, depending on auto_connect setting)
    hook: Option<PaymentHook>,
}

impl TransactionContext {
    /// Create a new transaction context
    fn new(transaction: Transaction, config: ChannelConfig) -> Self {
        Self {
            transaction,
            config,
            hook: None,
        }
    }

    /// Get the transaction ID
    pub fn id(&self) -> &str {
        &self.transaction.id
    }

    /// Get the channel ID for this transaction
    pub fn channel_id(&self) -> &str {
        &self.transaction.channel_id
    }

    /// Get the basket items
    pub fn basket(&self) -> &[BasketItem] {
        &self.transaction.basket
    }

    /// Get the payment details
    pub fn payment(&self) -> Option<&PaymentDetails> {
        self.transaction.payment.as_ref()
    }

    /// Get the payment amount as string (from payment details)
    pub fn amount(&self) -> &str {
        self.transaction
            .payment
            .as_ref()
            .map(|p| p.amount.as_str())
            .unwrap_or("0")
    }

    /// Get the currency (from payment details)
    pub fn currency(&self) -> &str {
        self.transaction
            .payment
            .as_ref()
            .map(|p| p.currency.as_str())
            .unwrap_or(defaults::CURRENCY)
    }

    /// Get the first product ID from basket (convenience method)
    pub fn product_id(&self) -> Option<&str> {
        self.transaction
            .basket
            .first()
            .map(|b| b.product_id.as_str())
    }

    /// Get the payer address (from payment details)
    pub fn payer(&self) -> Option<&str> {
        self.transaction.payment.as_ref().map(|p| p.payer.as_str())
    }

    /// Get the network (from payment details)
    pub fn network(&self) -> Option<&str> {
        self.transaction
            .payment
            .as_ref()
            .map(|p| p.network.as_str())
    }

    /// Get features for the first product in basket (convenience method)
    pub fn features(&self) -> Option<&serde_json::Value> {
        self.transaction.basket.first().map(|b| &b.features)
    }

    /// Create a payment hook scoped to this transaction's channel
    ///
    /// The hook can be used to both receive events on this transaction's
    /// channel and attach fulfillment data.
    pub fn hook(&self) -> PaymentHook {
        let mut config = self.config.clone();
        // Use transaction-specific stream ID for cursor tracking
        config.stream_id = Some(format!("tx-{}", self.transaction.id));

        PaymentHook::new(&self.transaction.channel_id, config)
    }

    /// Get access to a pre-connected hook (if processor was configured with auto_connect)
    pub fn connected_hook(&mut self) -> Option<&mut PaymentHook> {
        self.hook.as_mut()
    }
}

/// Payment stream configuration
#[derive(Debug, Clone)]
pub struct PaymentStreamConfig {
    /// Base configuration
    pub base: ChannelConfig,

    /// Whether to automatically connect hooks for each transaction
    pub auto_connect_hooks: bool,
}

impl PaymentStreamConfig {
    /// Create a new processor configuration
    pub fn new(endpoint: impl Into<String>, secret: impl Into<String>) -> Self {
        Self {
            base: ChannelConfig::new(endpoint).with_secret(secret),
            auto_connect_hooks: false,
        }
    }

    /// Enable automatic hook connection for each transaction
    pub fn with_auto_connect_hooks(mut self) -> Self {
        self.auto_connect_hooks = true;
        self
    }

    /// Set replay count
    pub fn with_replay(mut self, count: u32) -> Self {
        self.base.replay = Some(count);
        self
    }

    /// Set stream ID for cursor tracking
    pub fn with_stream_id(mut self, stream_id: impl Into<String>) -> Self {
        self.base.stream_id = Some(stream_id.into());
        self
    }

    /// Set the actor ID for attachments
    /// This becomes the outer key in attachments: attachments[actor_id][key] = data
    pub fn with_actor_id(mut self, actor_id: impl Into<String>) -> Self {
        self.base.actor_id = Some(actor_id.into());
        self
    }
}

/// Payment stream - listens for incoming transactions and spawns handlers
///
/// This is the Rust equivalent of the JavaScript SDK's EventReceiver.
/// It connects to the transaction stream and creates a TransactionContext
/// for each incoming transaction.
///
/// # Example
///
/// ```ignore
/// use moneymq_sdk::{PaymentStream, PaymentStreamConfig};
///
/// let config = PaymentStreamConfig::new("https://api.example.com", "your-secret-key")
///     .with_stream_id("my-payment-stream");
///
/// let mut stream = PaymentStream::new(config);
///
/// // Subscribe to transactions
/// let mut rx = stream.subscribe();
///
/// // Connect
/// stream.connect().await?;
///
/// while let Some(tx_ctx) = rx.recv().await {
///     println!("New transaction: {}", tx_ctx.id());
///
///     // Create a hook for this transaction
///     let mut hook = tx_ctx.hook();
///     hook.connect().await?;
///
///     // Process the payment...
///
///     // Attach fulfillment data
///     hook.attach("fulfillment", serde_json::json!({
///         "order_id": tx_ctx.id(),
///         "status": "fulfilled"
///     })).await?;
/// }
/// ```
pub struct PaymentStream {
    /// Configuration
    config: PaymentStreamConfig,

    /// Connection state
    state: Arc<RwLock<ConnectionState>>,

    /// Transaction sender
    tx_sender: mpsc::Sender<TransactionContext>,

    /// Transaction receiver (for subscription)
    tx_receiver: Option<mpsc::Receiver<TransactionContext>>,

    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,

    /// Reconnection attempt counter
    reconnect_attempts: Arc<RwLock<u32>>,
}

impl PaymentStream {
    /// Create a new processor with the given configuration
    pub fn new(config: PaymentStreamConfig) -> Self {
        let (tx_sender, tx_receiver) = mpsc::channel(256);

        Self {
            config,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            tx_sender,
            tx_receiver: Some(tx_receiver),
            shutdown_tx: None,
            reconnect_attempts: Arc::new(RwLock::new(0)),
        }
    }

    /// Get the current connection state
    pub fn state(&self) -> ConnectionState {
        *self.state.read()
    }

    /// Subscribe to incoming transactions
    ///
    /// Returns a receiver that will receive TransactionContext for each
    /// incoming transaction. This can only be called once (takes ownership
    /// of the receiver).
    pub fn subscribe(&mut self) -> Option<mpsc::Receiver<TransactionContext>> {
        self.tx_receiver.take()
    }

    /// Build the SSE URL for the transaction stream
    fn build_url(&self) -> String {
        let mut url = format!(
            "{}/payment/v1/channels/transactions",
            self.config.base.endpoint.trim_end_matches('/')
        );

        let mut params = Vec::new();

        // Token in query param for SSE auth
        if let Some(ref secret) = self.config.base.secret {
            params.push(format!("token={}", secret));
        }

        if let Some(replay) = self.config.base.replay {
            params.push(format!("replay={}", replay));
        }

        if let Some(ref stream_id) = self.config.base.stream_id {
            params.push(format!("stream_id={}", stream_id));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        url
    }

    /// Connect to the transaction stream and start receiving transactions
    pub async fn connect(&mut self) -> Result<()> {
        if self.state() == ConnectionState::Connected {
            return Ok(());
        }

        let _secret = self.config.base.secret.as_ref().ok_or_else(|| {
            PaymentStreamError::Authentication("Secret key required for processor".to_string())
        })?;

        self.set_state(ConnectionState::Connecting);

        let url = self.build_url();
        info!(url = %url, "PaymentStream connecting to transaction stream");

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        // Clone what we need for the task
        let tx_sender = self.tx_sender.clone();
        let state = Arc::clone(&self.state);
        let reconnect_attempts = Arc::clone(&self.reconnect_attempts);
        let config = self.config.clone();

        // Spawn connection task
        tokio::spawn(async move {
            Self::run_connection(
                url,
                tx_sender,
                state,
                reconnect_attempts,
                config,
                shutdown_rx,
            )
            .await;
        });

        Ok(())
    }

    /// Disconnect from the transaction stream
    pub async fn disconnect(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        self.set_state(ConnectionState::Disconnected);
        info!("PaymentStream disconnected");
    }

    /// Set the connection state
    fn set_state(&self, new_state: ConnectionState) {
        let mut state = self.state.write();
        *state = new_state;
    }

    /// Run the SSE connection loop
    async fn run_connection(
        url: String,
        tx_sender: mpsc::Sender<TransactionContext>,
        state: Arc<RwLock<ConnectionState>>,
        reconnect_attempts: Arc<RwLock<u32>>,
        config: PaymentStreamConfig,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) {
        loop {
            // Build request
            let client = reqwest::Client::new();
            let request = client.get(&url);

            // Create EventSource
            let mut es = EventSource::new(request).expect("Failed to create EventSource");

            // Update state to connected once we start receiving
            {
                let mut s = state.write();
                *s = ConnectionState::Connected;
            }
            {
                let mut attempts = reconnect_attempts.write();
                *attempts = 0;
            }
            info!("PaymentStream connected to transaction stream");

            // Process events
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("PaymentStream shutdown signal received");
                        es.close();
                        return;
                    }
                    event = es.next() => {
                        match event {
                            Some(Ok(SseEvent::Open)) => {
                                debug!("PaymentStream SSE connection opened");
                            }
                            Some(Ok(SseEvent::Message(msg))) => {
                                // Check if this is a transaction event
                                if msg.event == "transaction" {
                                    match serde_json::from_str::<Transaction>(&msg.data) {
                                        Ok(transaction) => {
                                            let amount = transaction.payment.as_ref().map(|p| p.amount.as_str()).unwrap_or("0");
                                            let currency = transaction.payment.as_ref().map(|p| p.currency.as_str()).unwrap_or(defaults::CURRENCY);
                                            info!(
                                                transaction_id = %transaction.id,
                                                channel_id = %transaction.channel_id,
                                                basket_items = %transaction.basket.len(),
                                                amount = %amount,
                                                currency = %currency,
                                                "Received transaction"
                                            );

                                            // Create transaction context
                                            let mut ctx = TransactionContext::new(
                                                transaction,
                                                config.base.clone(),
                                            );

                                            // Auto-connect hook if configured
                                            if config.auto_connect_hooks {
                                                let mut hook = ctx.hook();
                                                if let Err(e) = hook.connect().await {
                                                    warn!(
                                                        error = %e,
                                                        "Failed to auto-connect hook"
                                                    );
                                                } else {
                                                    ctx.hook = Some(hook);
                                                }
                                            }

                                            // Send to handler
                                            if tx_sender.send(ctx).await.is_err() {
                                                warn!("Transaction receiver dropped");
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            warn!(
                                                error = %e,
                                                data = %msg.data,
                                                "Failed to parse transaction"
                                            );
                                        }
                                    }
                                } else {
                                    // Other event types on the transaction stream
                                    debug!(
                                        event_type = %msg.event,
                                        data = %msg.data,
                                        "Received non-transaction event"
                                    );
                                }
                            }
                            Some(Err(e)) => {
                                error!(error = %e, "PaymentStream SSE error");
                                break;
                            }
                            None => {
                                info!("PaymentStream SSE stream ended");
                                break;
                            }
                        }
                    }
                }
            }

            // Connection lost - attempt reconnect if enabled
            es.close();

            if !config.base.auto_reconnect {
                {
                    let mut s = state.write();
                    *s = ConnectionState::Disconnected;
                }
                return;
            }

            // Check max attempts
            {
                let mut attempts = reconnect_attempts.write();
                *attempts += 1;
                if config.base.max_reconnect_attempts > 0
                    && *attempts >= config.base.max_reconnect_attempts
                {
                    error!(
                        attempts = *attempts,
                        "PaymentStream max reconnection attempts reached"
                    );
                    let mut s = state.write();
                    *s = ConnectionState::Disconnected;
                    return;
                }
            }

            {
                let mut s = state.write();
                *s = ConnectionState::Reconnecting;
            }

            info!(
                delay_ms = config.base.reconnect_delay_ms,
                "PaymentStream scheduling reconnection"
            );

            tokio::time::sleep(tokio::time::Duration::from_millis(
                config.base.reconnect_delay_ms,
            ))
            .await;
        }
    }
}

impl Drop for PaymentStream {
    fn drop(&mut self) {
        // Signal shutdown synchronously
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.try_send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_url() {
        let config = PaymentStreamConfig::new("https://api.example.com", "my-secret")
            .with_stream_id("my-stream");
        let stream = PaymentStream::new(config);
        assert_eq!(
            stream.build_url(),
            "https://api.example.com/payment/v1/channels/transactions?token=my-secret&stream_id=my-stream"
        );
    }

    #[test]
    fn test_build_url_with_replay() {
        let config =
            PaymentStreamConfig::new("https://api.example.com", "my-secret").with_replay(10);
        let stream = PaymentStream::new(config);
        assert_eq!(
            stream.build_url(),
            "https://api.example.com/payment/v1/channels/transactions?token=my-secret&replay=10"
        );
    }

    #[test]
    fn test_transaction_context_hook() {
        let tx = Transaction {
            id: "tx-123".to_string(),
            channel_id: "channel-456".to_string(),
            basket: vec![BasketItem {
                product_id: "prod-789".to_string(),
                experiment_id: None,
                features: serde_json::Value::default(),
                quantity: 1,
            }],
            payment: Some(PaymentDetails {
                payer: "payer-address".to_string(),
                transaction_id: "tx-123".to_string(),
                amount: "1000".to_string(),
                currency: defaults::CURRENCY.to_string(),
                network: defaults::NETWORK.to_string(),
            }),
            metadata: Default::default(),
        };

        let config = ChannelConfig::new("https://api.example.com").with_secret("secret");
        let ctx = TransactionContext::new(tx, config);

        assert_eq!(ctx.id(), "tx-123");
        assert_eq!(ctx.channel_id(), "channel-456");
        assert_eq!(ctx.amount(), "1000");
        assert_eq!(ctx.currency(), defaults::CURRENCY);
        assert_eq!(ctx.product_id(), Some("prod-789"));
        assert_eq!(ctx.payer(), Some("payer-address"));
        assert_eq!(ctx.network(), Some(defaults::NETWORK));

        let hook = ctx.hook();
        assert_eq!(hook.channel_id(), "channel-456");
    }
}
