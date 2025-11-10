# Surfpool Example

A simple TypeScript example that uses the Stripe library to fetch and display products, prices, and billing meters.

## Setup

1. Install dependencies:
```bash
npm install
```

2. Create a `.env` file with your Stripe API key:
```bash
STRIPE_API_KEY=sk_test_...
```

The application will automatically load environment variables from the `.env` file.

## Run

**Development mode** (with tsx - no build needed):
```bash
npm run dev
```

**Build and run**:
```bash
npm run build
npm start
```

## What it does

- Fetches up to 5 products from your Stripe account
- For each product, displays its prices
- Fetches billing meters if available
- Shows formatted output with pricing information
