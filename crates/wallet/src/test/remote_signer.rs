use solana_keypair::{Keypair, Signer};
use solana_pubkey::Pubkey;

use axum::{
    Json, Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};

pub struct RemoteSigner {
    pub port: u16,
}

impl RemoteSigner {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub async fn start_service(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Start a service that listens for signing requests in a background task
        println!("Starting RemoteSigner service at {}", self.port);

        let bind_addr = format!("0.0.0.0:{}", self.port);
        let keypair_bytes = Keypair::new().to_base58_string();

        let _handle = tokio::spawn(async move {
            println!("Starting server");
            let app = create_router(keypair_bytes);
            println!("Created router");
            let tcp_listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
            axum::serve(tcp_listener, app).await.unwrap();
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        Ok(())
    }
}

impl RemoteSigner {
    pub async fn get_pubkey(&self) -> Pubkey {
        println!("Fetching pubkey from RemoteSigner service");
        let client = reqwest::Client::new();
        let res = client
            .get(&format!("http://127.0.0.1:{}/pubkey", self.port))
            .send()
            .await
            .unwrap()
            .json::<Pubkey>()
            .await
            .unwrap();
        res
    }

    pub async fn async_sign_message(&self, message: &[u8]) -> solana_keypair::Signature {
        let client = reqwest::Client::new();
        let res = client
            .post(format!("http://127.0.0.1:{}/sign_message", self.port))
            .json(&message)
            .send()
            .await
            .unwrap()
            .json::<solana_keypair::Signature>()
            .await
            .unwrap();
        res
    }
}

impl Signer for RemoteSigner {
    fn pubkey(&self) -> Pubkey {
        // Check if we're in an async context
        if tokio::runtime::Handle::try_current().is_ok() {
            // We're in an async context, so we need to use a blocking approach
            let port = self.port;
            std::thread::scope(|s| {
                let handle = s.spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async move {
                        let client = reqwest::Client::new();
                        client
                            .get(&format!("http://127.0.0.1:{}/pubkey", port))
                            .send()
                            .await
                            .unwrap()
                            .json::<Pubkey>()
                            .await
                            .unwrap()
                    })
                });
                handle.join().unwrap()
            })
        } else {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(self.get_pubkey())
        }
    }

    fn sign_message(&self, message: &[u8]) -> solana_keypair::Signature {
        if tokio::runtime::Handle::try_current().is_ok() {
            let port = self.port;
            let message = message.to_vec();
            std::thread::scope(|s| {
                let handle = s.spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async move {
                        let client = reqwest::Client::new();
                        client
                            .post(&format!("http://127.0.0.1:{}/sign_message", port))
                            .json(&message)
                            .send()
                            .await
                            .unwrap()
                            .json::<solana_keypair::Signature>()
                            .await
                            .unwrap()
                    })
                });
                handle.join().unwrap()
            })
        } else {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(self.async_sign_message(message))
        }
    }

    fn try_pubkey(&self) -> Result<Pubkey, solana_transaction::SignerError> {
        Ok(self.pubkey())
    }

    fn try_sign_message(
        &self,
        message: &[u8],
    ) -> Result<solana_keypair::Signature, solana_transaction::SignerError> {
        Ok(self.sign_message(message))
    }

    fn is_interactive(&self) -> bool {
        false
    }
}

pub async fn pubkey(State(state): State<String>) -> impl IntoResponse {
    let keypair = Keypair::from_base58_string(&state);
    let pubkey = keypair.pubkey();
    Json(pubkey)
}

pub async fn try_pubkey(State(state): State<String>) -> impl IntoResponse {
    let keypair = Keypair::from_base58_string(&state);
    let pubkey = keypair.try_pubkey().unwrap();
    Json(pubkey)
}

pub async fn sign_message(
    State(state): State<String>,
    Json(request): Json<Vec<u8>>,
) -> impl IntoResponse {
    let keypair = Keypair::from_base58_string(&state);
    let signature = keypair.sign_message(&request);
    Json(signature)
}

pub async fn try_sign_message(
    State(state): State<String>,
    Json(request): Json<Vec<u8>>,
) -> impl IntoResponse {
    let keypair = Keypair::from_base58_string(&state);
    let signature = keypair.try_sign_message(&request).unwrap();
    Json(signature)
}

pub async fn is_interactive(State(state): State<String>) -> impl IntoResponse {
    let keypair = Keypair::from_base58_string(&state);
    let interactive = keypair.is_interactive();
    Json(interactive)
}

fn create_router(keypair_bytes: String) -> Router {
    Router::new()
        .route("/pubkey", get(pubkey))
        .route("/try_pubkey", get(try_pubkey))
        .route("/sign_message", post(sign_message))
        .route("/try_sign_message", post(try_sign_message))
        .route("/is_interactive", get(is_interactive))
        .with_state(keypair_bytes)
}
