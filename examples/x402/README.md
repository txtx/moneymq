---
title: x402 Integration with MoneyMQ - Complete Demo Guide
description: Learn how to integrate the x402 payment protocol with MoneyMQ, enabling micropayments for API access with zero configuration.
date: 2025-11-16
---

# Solana x402 Protocol Integration with MoneyMQ

## What You'll Build

This guide walks you through implementing a complete x402 (HTTP 402 Payment Required) integration with MoneyMQ. By the end, you'll have a working system where:

- APIs can charge micropayments for access using the x402 protocol
- Users pay in USDC without needing SOL for gas fees
- MoneyMQ handles transaction fees as the facilitator
- Payments are settled on a local Solana blockchain
- Everything runs locally with pre-seeded test accounts

The final result will be a fully functional payment-protected API:

```shell
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
X402 + MONEYMQ PAYMENT FLOW DEMONSTRATION
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

[1/4] Initializing payment signer
  → Network: solana
  → Payer address: DVGD...YiNT
  ✓ Signer initialized

[2/4] Attempting to access protected endpoint without payment
  → GET http://localhost:4021/protected
  → Response: 402 Payment Required
  ✅ Status code: 402

[3/4] Accessing protected endpoint with x402 payment
  → Using x402 fetch wrapper
  → Payment will be processed via MoneyMQ facilitator
  → Transaction submitted to local Solana
  ✅ Status code: 200

[4/4] Processing response data
  ✓ Payment response decoded

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
SUCCESS: Payment completed and API accessed
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Response Data:
{
    "data": {
        "message": "Protected endpoint accessed successfully",
        "timestamp": "2025-11-16T20:14:04.242Z"
    },
    "status_code": 200,
    "payment_response": {
        "transaction": "5ULZpdeThaMAy6hcEGfAoMFqJqPpCtxdCxb6JYUV6nA4...",
        "success": true,
        "network": "solana"
    }
}
```

## What is x402?

[x402](https://www.x402.org/) is an open payment standard that enables seamless micropayments for API access. Instead of traditional subscription models or API keys, x402 allows servers to charge for individual API calls, creating true pay-per-use infrastructure.

Key benefits of x402:
- **Instant Micropayments**: Pay fractions of a cent per API call
- **Enable AI agents to pay for API calls**: AI agents can autonomously pay for services
- **No Subscriptions**: Users only pay for what they use
- **Web3 Payments**: Transparent, verifiable payments on-chain
- **Standard HTTP**: Works with existing web infrastructure using HTTP 402 status code

Servers using x402 return an HTTP 402 status code when payment is required. To access protected endpoints, clients must include a valid payment in the `X-PAYMENT` header. x402 relies on "Facilitators" to verify and settle transactions so servers don't need to directly interact with blockchain infrastructure.

### Understanding Facilitators

Facilitators are specialized services that handle blockchain payments on behalf of API servers.

**What Facilitators Do:**
- **Verify Payments**: Validate that payment payloads are correctly formed and sufficient
- **Abstract Complexity**: Remove the need for servers to interact with blockchain infrastructure
- **Settle Transactions**: Submit validated transactions to Solana

**MoneyMQ acts as your facilitator**, handling all payment verification and settlement automatically.

## What is MoneyMQ?

MoneyMQ is a complete payment infrastructure solution that provides:

- **Built-in Facilitator**: x402 payment verification and settlement
- **Local Blockchain**: Embedded Solana validator for development
- **Pre-seeded Accounts**: Test accounts with USDC ready to use
- **Fee Management**: Automatic handling of transaction fees
- **Zero Configuration**: Everything works out of the box

In this demo, MoneyMQ handles all the complexity, letting you focus on building your API.

### Powered by Kora

MoneyMQ's x402 facilitator is built on top of [Kora](https://github.com/solana-foundation/kora), the Solana Foundation's gasless transaction signing infrastructure. Kora provides the core capabilities that enable MoneyMQ's facilitator functionality:

- **Transaction Signing**: Kora signs transactions on behalf of users, abstracting away gas fees
- **Policy Engine**: Validates transactions against configurable security policies
- **Signer Backends**: Supports multiple secure key storage options (memory, Vault, Turnkey, Privy)
- **Fee Sponsorship**: Allows users to pay in USDC without needing SOL for transaction fees

MoneyMQ wraps Kora with additional tooling for development, including:
- Automatic account seeding and USDC distribution
- Local Solana validator integration
- x402-specific facilitator endpoints
- Web-based integration UI

**Credits:** This example and the x402 facilitator implementation are heavily inspired by the [original Kora x402 demo](https://github.com/solana-foundation/kora/tree/main/docs/x402/demo). We're grateful to the Kora team for pioneering this integration pattern.

## Architecture Overview

The x402 + MoneyMQ integration consists of three components:

Complete Payment Flow:
1. Client requests protected resource → API returns 402 Payment Required
2. Client creates payment transaction with x402 fetch wrapper
3. Client sends payment to MoneyMQ Facilitator for verification
4. MoneyMQ validates and settles transaction on local Solana
5. Transaction confirmed, MoneyMQ notifies API
6. API returns protected content with payment receipt

### Component Breakdown

1. **MoneyMQ Sandbox** (Ports 7781, 8899, 8488)
   - x402 Facilitator (port 7781): Verifies and settles payments
   - Local Solana Validator (port 8899): Local blockchain
   - Integration UI (port 8488): View pre-seeded accounts

2. **Protected API** (Port 4021)
   - Demo API server with payment-protected endpoints
   - Uses x402-express middleware for payment handling
   - Returns data only after successful payment

3. **Client Application**
   - Demonstrates x402 fetch wrapper usage
   - Signs transactions with user's private key
   - Automatically handles payment flow

## Prerequisites

Before starting, ensure you have:

- [**Rust**](https://www.rust-lang.org/tools/install) (latest stable version)
- [**Node.js**](https://nodejs.org/en/download) (v18 or later)
- [**pnpm**](https://pnpm.io/installation) (latest version)
- Basic understanding of [Solana transactions](https://solana.com/docs/core/transactions)

## Installation

### Install MoneyMQ CLI

```bash
git clone https://github.com/txtx/moneymq.git
cd moneymq
cargo moneymq-install
```

This installs the `moneymq` command-line tool globally.

### Navigate to Demo Directory

```bash
cd examples/x402
```

### Install Dependencies

Install Node.js dependencies for the API and client:

```bash
# Install API dependencies
cd api && pnpm install && cd ..

# Install client dependencies
cd client && pnpm install && cd ..
```

## Running the Demo

You'll need three terminal windows.

### Terminal 1: Start MoneyMQ Sandbox

From the project root (not the x402 directory):

```bash
moneymq sandbox
```

You should see:

```
# Payment API (protocol: x402, paying with 3iR5o1byE3RaMdZ5nWv5dEajSmTJsNUTFVUcnRumCgMU)
   GET http://localhost:7781/supported
  POST http://localhost:7781/verify
  POST http://localhost:7781/settle

MoneyMQ Studio:: http://localhost:8488 - Press Ctrl+C to stop
```

**View Pre-Seeded Accounts:**
Open http://localhost:8488/integrate in your browser to see all test accounts with their USDC balances and private keys.

The sandbox provides:
- **Facilitator** at `http://localhost:7781`
- **Local Solana RPC** at `http://localhost:8899`
- **Pre-seeded test accounts** with 2000 USDC each

### Terminal 2: Start Protected API

From the `examples/x402/api` directory:

```bash
pnpm start
```

You should see:
```
Server listening at http://localhost:4021
```

### Terminal 3: Run Client Demo

From the `examples/x402/client` directory:

```bash
pnpm start
```

You should see the complete payment flow demonstration!

## Understanding the Implementation

Here's what happens during a successful payment flow:

1. **Client Request** → API returns 402 with payment requirements
2. **Payment Creation** → Client creates Solana transaction
3. **Payment Submission** → Client sends request with payment in `X-PAYMENT` header
4. **Verification** → MoneyMQ verifies the payment transaction
5. **Settlement** → MoneyMQ settles on local Solana blockchain
6. **Access Granted** → API returns protected content with payment receipt

### The Protected API

The API server (`api/src/api.ts`) uses x402-express middleware to protect endpoints:

```typescript
import { paymentMiddleware } from "x402-express";

const PAYOUT_RECIPIENT_ADDRESS = process.env.PAYOUT_RECIPIENT_ADDRESS;
const FACILITATOR_URL = "http://localhost:7781";

app.use(
  paymentMiddleware(
    PAYOUT_RECIPIENT_ADDRESS,  // Where payments should go
    {
      "GET /protected": {
        price: "$0.0001",        // Price in USD
        network: "solana",
      },
    },
    {
      url: FACILITATOR_URL,      // MoneyMQ facilitator
    }
  ),
);

app.get("/protected", (req, res) => {
  res.json({
    message: "Protected endpoint accessed successfully",
    timestamp: new Date().toISOString(),
  });
});
```

The middleware:
- Intercepts requests to `/protected`
- Returns 402 status if payment is missing
- Validates payments via MoneyMQ facilitator
- Allows access after successful payment

### The Client Application

The client (`client/src/index.ts`) demonstrates the x402 payment flow:

```typescript
import { wrapFetchWithPayment, createSigner } from "x402";

// Create a signer from private key
const payer = await createSigner("solana", PAYER_PRIVATE_KEY);

// Wrap fetch with x402 payment capabilities
const fetchWithPayment = wrapFetchWithPayment(
  fetch,
  payer,
  undefined,
  undefined,
  {
    svmConfig: { rpcUrl: "http://localhost:8899" }
  }
);

// First attempt: Regular fetch (will fail with 402)
const expect402Response = await fetch(PROTECTED_API_URL);
console.log(`Status: ${expect402Response.status}`); // 402

// Second attempt: Fetch with payment (succeeds)
const response = await fetchWithPayment(PROTECTED_API_URL);
console.log(`Status: ${response.status}`); // 200
```

The x402 fetch wrapper:
- Detects 402 responses
- Creates payment transaction based on requirements
- Signs with user's private key
- Submits to MoneyMQ for verification and settlement
- Retries with payment proof
- Returns successful response

## Configuration

### Environment Variables

The `.env` file in `examples/x402` contains:

```bash
# MoneyMQ facilitator URL
FACILITATOR_URL=http://localhost:7781

# API configuration
PROTECTED_API_URL=http://localhost:4021/protected
API_PORT=4021

# The address that receives payments
PAYOUT_RECIPIENT_ADDRESS=62CwiUCt7o2ygfSrBmL941X2XvWqNdcStBk9rWDWMiep

# Customer wallet (pre-seeded with 2000 USDC)
PAYER_ADDRESS=DVGD278xQEJxKhYZBPkcPEbcncFV6HsdwUoe45LNYiNT
PAYER_PRIVATE_KEY=4SQ3kBvLqeAXoRMhXgRUG27ehizp6R5vQMhu4HAM3R8g...

# Enable debug logging
DEBUG=false
```

**Note:** These addresses are automatically created by MoneyMQ sandbox with pre-seeded USDC.

## Debugging

### Enable Debug Logs

Set `DEBUG=true` in your `.env` file to see detailed logs:

```bash
DEBUG=true
```

This shows:
- Request/response details
- Payment transaction data
- Error messages with full context
- Transaction signatures

### Common Issues

**Port Already in Use:**
```bash
# Check what's using port 7781 (facilitator)
lsof -i :7781

# Check what's using port 8899 (Solana RPC)
lsof -i :8899

# Check what's using port 4021 (API)
lsof -i :4021
```

**USDC Balance Too Low:**
Visit http://localhost:8488/integrate to see account balances. MoneyMQ seeds accounts with 2000 USDC, which should be sufficient for testing.

**RPC Connection Errors:**
Ensure MoneyMQ sandbox is running and the local Solana validator is healthy:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getHealth"}
'
```

## Wrapping Up

Congratulations! 🎉 You've successfully implemented a complete x402 payment flow with MoneyMQ. This demonstration shows how:

- **x402 Protocol** enables frictionless API monetization
- **MoneyMQ** provides complete payment infrastructure out of the box
- **Users** can pay for API access without managing blockchain complexity

This architecture creates a foundation for:
- AI Agent marketplaces
- Pay-per-use APIs
- Micropayment content platforms
- Usage-based SaaS pricing
- Any service requiring instant, verifiable payments

### Next Steps

- **Customize Pricing**: Modify the API to charge different amounts for different endpoints
- **Add More Endpoints**: Protect multiple routes with different pricing tiers
- **Build Your Own API**: Create a real service that monetizes through x402
- **Deploy to Production**: Use MoneyMQ with real Solana networks

## Additional Resources

### x402 Protocol
- [x402 Documentation](https://x402.gitbook.io/x402/)
- [x402 GitHub](https://github.com/coinbase/x402)
- [x402 TypeScript SDK](https://www.npmjs.com/package/x402)

### MoneyMQ
- [MoneyMQ GitHub](https://github.com/txtx/moneymq)
- [MoneyMQ Documentation](../../docs/)

### Kora
- [Kora GitHub](https://github.com/solana-foundation/kora)
- [Kora x402 Demo](https://github.com/solana-foundation/kora/tree/main/docs/x402/demo) (Original implementation this demo is based on)
- [Kora Configuration Guide](https://github.com/solana-foundation/kora/blob/main/docs/operators/CONFIGURATION.md)
- [Kora TypeScript SDK](https://github.com/solana-foundation/kora/tree/main/sdks/ts)

### Solana
- [Solana Documentation](https://solana.com/docs)
- [SPL Token Program](https://spl.solana.com/token)

## Support

Need help?
- Open issues on the [MoneyMQ GitHub repository](https://github.com/txtx/moneymq)
- Ask questions on [Solana Stack Exchange](https://solana.stackexchange.com/) with `x402` tag
