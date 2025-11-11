use serde_json::json;
use solana_client::{rpc_client::RpcClient, rpc_request::RpcRequest};
use solana_keypair::Pubkey;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct SetAccountRequest {
    pubkey: Pubkey,
    lamports: Option<u64>,
    data: Option<Vec<u8>>,
    owner: Option<Pubkey>,
}

impl SetAccountRequest {
    pub fn new(pubkey: Pubkey) -> Self {
        Self {
            pubkey,
            lamports: None,
            data: None,
            owner: None,
        }
    }
    pub fn lamports(mut self, lamports: u64) -> Self {
        self.lamports = Some(lamports);
        self
    }

    pub fn data(mut self, data: Vec<u8>) -> Self {
        self.data = Some(data);
        self
    }

    pub fn owner(mut self, owner: Pubkey) -> Self {
        self.owner = Some(owner);
        self
    }

    pub fn to_params(self) -> serde_json::Value {
        let mut account_data = json!({});

        if let Some(lamports) = self.lamports {
            account_data["lamports"] = json!(lamports);
        }
        if let Some(data) = &self.data {
            account_data["data"] = json!(data);
        }
        if let Some(owner) = &self.owner {
            account_data["owner"] = json!(owner.to_string());
        }

        json!([self.pubkey.to_string(), account_data])
    }
}

/// Sets account data for a given account on the Surfnet local validator
pub fn surfnet_set_account(rpc_client: &RpcClient, req: SetAccountRequest) -> Result<(), String> {
    let pubkey = req.pubkey;
    let _ = rpc_client
        .send::<serde_json::Value>(
            RpcRequest::Custom {
                method: "surfnet_setAccount",
            },
            req.to_params(),
        )
        .map_err(|e| format!("Failed to set account data for {}: {}", pubkey, e))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SetTokenAccountRequest {
    pubkey: Pubkey,
    mint: Pubkey,
    token_program: Pubkey,
    amount: Option<u64>,
}
impl SetTokenAccountRequest {
    pub fn new(pubkey: Pubkey, mint: Pubkey, token_program: Pubkey) -> Self {
        Self {
            pubkey,
            mint,
            token_program,
            amount: None,
        }
    }

    pub fn amount(mut self, amount: u64) -> Self {
        self.amount = Some(amount);
        self
    }

    pub fn to_params(self) -> serde_json::Value {
        let mut account_data = json!({});

        if let Some(amount) = &self.amount {
            account_data["amount"] = json!(amount);
        }

        json!([
            self.pubkey.to_string(),
            self.mint.to_string(),
            account_data,
            self.token_program.to_string()
        ])
    }
}

/// Sets token account data for a given account on the Surfnet local validator
pub fn surfnet_set_token_account(
    rpc_client: &RpcClient,
    req: SetTokenAccountRequest,
) -> Result<(), String> {
    let pubkey = req.pubkey;
    let mint = req.mint;
    debug!("Setting token account with payload: {:?}", req);

    let _ = rpc_client
        .send::<serde_json::Value>(
            RpcRequest::Custom {
                method: "surfnet_setTokenAccount",
            },
            req.to_params(),
        )
        .map_err(|e| {
            format!(
                "Failed to set token account data for {} with mint {}: {}",
                pubkey, mint, e
            )
        })?;
    Ok(())
}
