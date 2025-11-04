use solana_keypair::Pubkey;

#[derive(Debug, Clone)]
pub enum ServerConfig {
    SolanaSurfnet(SolanaSurfnetServerConfig),
    SolanaMainnet(SolanaMainnetServerConfig),
}

#[derive(Debug, Clone)]
pub struct SolanaSurfnetServerConfig {
    pub payment_recipient: Pubkey,
}

#[derive(Debug, Clone)]
pub struct SolanaMainnetServerConfig {
    pub payment_recipient: Pubkey,
}
