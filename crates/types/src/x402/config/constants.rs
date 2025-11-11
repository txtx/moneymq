/// Default binding address for facilitator and validator
pub const DEFAULT_BINDING_ADDRESS: &str = "0.0.0.0";

/// Default port for the facilitator service
pub const DEFAULT_FACILITATOR_PORT: u16 = 8080;

/// Default RPC port for the validator
pub const DEFAULT_RPC_PORT: u16 = 8899;

/// Default WebSocket port for the validator
pub const DEFAULT_WS_PORT: u16 = 8900;

/// Header comment for payments configuration section in manifest files
pub const PAYMENTS_CONFIG_HEADER: &str = "\
# Payment configuration for accepting crypto payments via x402 protocol
# Learn more: https://docs.moneymq.co/payments
# Uncomment and configure to enable payments:";

/// Complete default payments footer for manifest files
pub const DEFAULT_PAYMENTS_FOOTER: &str = "\
# Payment configuration for accepting crypto payments via x402 protocol
# Learn more: https://docs.moneymq.co/payments
# Uncomment and configure to enable payments:
# payments:
#   stablecoins:
#     # Protocol: x402 (HTTP 402 Payment Required)
#     protocol: x402
#     description: \"Solana stablecoin payments\"
#
#     # Production facilitator
#     facilitator:
#       service_url: https://facilitator.moneymq.co
#
#     # Accepted networks and currencies
#     accepted:
#       solana:
#         recipient: your_solana_address_here
#         currencies:
#           - USDC
#           - USDT
#
#     # Sandbox configuration for local development
#     sandboxes:
#       default:
#         description: \"Local development sandbox\"
#
#         # Embedded facilitator (runs locally)
#         facilitator:
#           binding_address: 0.0.0.0
#           binding_port: 8080
#           supported:
#             solana:
#               currencies:
#                 - USDC
#               fee: 0
#               # Optional: Pre-funded test accounts
#               # user_accounts:
#               #   - user1_account
#               #   - user2_account
#
#         # Local Solana validator configuration
#         validator:
#           binding_address: 0.0.0.0
#           rpc_binding_port: 8899
#           ws_binding_port: 8900";
