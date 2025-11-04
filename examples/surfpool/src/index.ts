import dotenv from "dotenv";
import { createStripeClient } from "./moneymq/createStripeClient";
import { createSigner } from "x402-fetch";
import Stripe from "stripe";
import type { MultiNetworkSigner } from "x402/types";

// Load environment variables from .env file
dotenv.config();

// Initialize Stripe client with real X402 payment signing
async function initializeClient(): Promise<Stripe> {
  console.log("🔧 Initializing Stripe client with X402 payment middleware");

  let walletClient: MultiNetworkSigner | undefined;

  if (process.env.PAYER_PRIVATE_KEY) {
    // Create real signer from private key
    const payer = await createSigner("solana", process.env.PAYER_PRIVATE_KEY, {
      svmConfig: "http://localhost:8899",
    });
    walletClient = { svm: payer };
    console.log(
      `💰 Wallet initialized: ${payer.address.slice(0, 4)}...${payer.address.slice(-4)}`,
    );
    console.log("✅ Using real payment signatures\n");
  } else {
    console.warn(
      "⚠️  WARNING: No PAYER_PRIVATE_KEY provided. Using mock payments.",
    );
    console.warn(
      "   This example will fail at payment verification with the facilitator.",
    );
    console.warn(
      "   Set PAYER_PRIVATE_KEY in your .env file for real payments.\n",
    );
  }

  // Initialize Stripe with X402 payment support
  return createStripeClient(
    process.env.STRIPE_SANDBOX_SECRET_KEY || "",
    walletClient, // Pass real signer or undefined for mock
    {
      apiVersion: "2025-10-29.clover" as any, // Using sandbox version
      host: "localhost",
      port: 8488,
      protocol: "http",
    },
    {
      svmConfig: "http://localhost:8899",
    },
  );
}

async function listCatalog() {
  console.log("🏄 Surfpool Stripe Example\n");

  const stripe = await initializeClient();

  try {
    // Fetch products
    console.log("📦 Fetching products...");
    const products = await stripe.products.list({ limit: 5 });

    console.log(`\nFound ${products.data.length} products:\n`);

    for (const product of products.data) {
      console.log(`  • ${product.name} (${product.id})`);
      console.log(`    Active: ${product.active}`);
      console.log(
        `    Created: ${new Date(product.created * 1000).toLocaleDateString()}`,
      );

      // Fetch prices for this product
      const prices = await stripe.prices.list({
        product: product.id,
        limit: 5,
      });

      if (prices.data.length > 0) {
        console.log(`    Prices:`);
        for (const price of prices.data) {
          const amount = price.unit_amount
            ? `$${(price.unit_amount / 100).toFixed(2)}`
            : "custom";
          const interval = price.recurring
            ? `/${price.recurring.interval}`
            : "";
          console.log(`      - ${amount}${interval} (${price.id})`);
        }
      }
      console.log("");
    }

    // Fetch billing meters if any
    console.log("📊 Fetching billing meters...");
    const meters = await stripe.billing.meters.list({ limit: 5 });

    if (meters.data.length > 0) {
      console.log(`\nFound ${meters.data.length} meters:\n`);
      for (const meter of meters.data) {
        console.log(`  • ${meter.display_name} (${meter.id})`);
        console.log(`    Event: ${meter.event_name}`);
        console.log(`    Status: ${meter.status}`);
        console.log("");
      }
    } else {
      console.log("  No meters found\n");
    }
  } catch (error) {
    if (error instanceof Error) {
      console.error("❌ Error:", error.message);

      if (error.message.includes("API key")) {
        console.error("\n💡 Tip: Set your Stripe API key:");
        console.error("   export STRIPE_API_KEY=sk_test_...\n");
      }
    }
    process.exit(1);
  }
}

/**
 * Complete subscription flow for a user purchasing Surfpool Max
 */
async function purchaseSubscription() {
  console.log("🛒 Surfpool Max Subscription Purchase Flow\n");

  const stripe = await initializeClient();

  try {
    // Step 1: Find the Surfpool Max product and its price
    console.log("1️⃣ Finding Surfpool Max product...");
    const products = await stripe.products.list({ limit: 100 });
    const surfpoolMax = products.data.find((p) => p.name === "Surfpool Max");

    if (!surfpoolMax) {
      throw new Error("Surfpool Max product not found");
    }

    console.log(`   ✓ Found: ${surfpoolMax.name} (${surfpoolMax.id})\n`);

    // Step 2: Get the price for Surfpool Max (using the $499/month price)
    console.log("2️⃣ Getting pricing information...");
    const prices = await stripe.prices.list({
      product: surfpoolMax.id,
      limit: 10,
    });

    // Find the $499/month price (49900 cents)
    const selectedPrice = prices.data.find((p) => p.unit_amount === 49900);

    if (!selectedPrice) {
      throw new Error("Surfpool Max $499/month price not found");
    }

    console.log(
      `   ✓ Selected: $${(selectedPrice.unit_amount! / 100).toFixed(2)}/${selectedPrice.recurring?.interval}`,
    );
    console.log(`   ✓ Price ID: ${selectedPrice.id}\n`);

    // Step 3: Create a customer
    console.log("3️⃣ Creating customer...");
    const customer = await stripe.customers.create({
      email: "john.doe@example.com",
      name: "John Doe",
      metadata: {
        user_id: "user_12345",
      },
    });

    console.log(`   ✓ Customer created: ${customer.id}`);
    console.log(`   ✓ Email: ${customer.email}\n`);

    // Step 4: Create a payment method (simulating a card)
    console.log("4️⃣ Setting up payment method...");
    const paymentMethod = await stripe.paymentMethods.create({
      type: "card",
      card: {
        token: "tok_visa", // Test token for Stripe test mode
      },
    });

    console.log(`   ✓ Payment method created: ${paymentMethod.id}`);
    console.log(`   ✓ Card ending in: ${paymentMethod.card?.last4}\n`);

    // Step 5: Attach payment method to customer
    console.log("5️⃣ Attaching payment method to customer...");
    await stripe.paymentMethods.attach(paymentMethod.id, {
      customer: customer.id,
    });

    console.log(`   ✓ Payment method attached\n`);

    // Step 6: Set as default payment method
    console.log("6️⃣ Setting default payment method...");
    await stripe.customers.update(customer.id, {
      invoice_settings: {
        default_payment_method: paymentMethod.id,
      },
    });

    console.log(`   ✓ Default payment method set\n`);

    // Step 7: Create the subscription
    console.log("7️⃣ Creating subscription...");
    const subscription = await stripe.subscriptions.create({
      customer: customer.id,
      items: [
        {
          price: selectedPrice.id,
        },
      ],
      payment_settings: {
        payment_method_types: ["card"],
      },
      expand: ["latest_invoice.payment_intent"],
    });

    console.log(`   ✓ Subscription created: ${subscription.id}`);
    console.log(`   ✓ Status: ${subscription.status}`);
    console.log(
      `   ✓ Current period: ${new Date((subscription as any).current_period_start * 1000).toLocaleDateString()} - ${new Date((subscription as any).current_period_end * 1000).toLocaleDateString()}`,
    );

    if (subscription.latest_invoice) {
      const invoice =
        typeof subscription.latest_invoice === "string"
          ? subscription.latest_invoice
          : subscription.latest_invoice.id;
      console.log(`   ✓ Invoice: ${invoice}`);
    }

    console.log("\n✅ Subscription purchase complete!\n");

    // Step 8: Retrieve and display subscription details
    console.log("📋 Subscription Summary:");
    console.log(`   Customer: ${customer.name} (${customer.email})`);
    console.log(`   Product: ${surfpoolMax.name}`);
    console.log(
      `   Price: $${(selectedPrice.unit_amount! / 100).toFixed(2)}/${selectedPrice.recurring?.interval}`,
    );
    console.log(`   Status: ${subscription.status}`);
    console.log(`   Subscription ID: ${subscription.id}`);

    return {
      customer,
      subscription,
      product: surfpoolMax,
      price: selectedPrice,
    };
  } catch (error) {
    if (error instanceof Error) {
      console.error("❌ Error:", error.message);
    }
    throw error;
  }
}

/**
 * Usage-based billing example - Recording meter events for RPC requests
 */
async function usageBasedBilling() {
  console.log("📊 Usage-Based Billing Example\n");

  const stripe = await initializeClient();

  try {
    // Step 1: Get the billing meter
    console.log("1️⃣ Fetching billing meter...");
    const meters = await stripe.billing.meters.list({ limit: 10 });
    const rpcMeter = meters.data.find(
      (m) => m.event_name === "surfnet_rpc_requests",
    );

    if (!rpcMeter) {
      throw new Error("Surfnet RPC requests meter not found");
    }

    console.log(`   ✓ Found: ${rpcMeter.display_name}`);
    console.log(`   ✓ Meter ID: ${rpcMeter.id}`);
    console.log(`   ✓ Event: ${rpcMeter.event_name}\n`);

    // Step 2: Create a customer for usage tracking
    console.log("2️⃣ Creating customer...");
    const customer = await stripe.customers.create({
      email: "api.user@example.com",
      name: "API User",
      metadata: {
        user_id: "api_user_789",
      },
    });

    console.log(`   ✓ Customer created: ${customer.id}`);
    console.log(`   ✓ Email: ${customer.email}\n`);

    // Step 3: Simulate usage - Record meter events for RPC requests
    console.log("3️⃣ Recording usage events...");
    const usageEvents = [];

    // Simulate 5 RPC requests
    for (let i = 1; i <= 5; i++) {
      const event = await stripe.billing.meterEvents.create({
        event_name: "surfnet_rpc_requests",
        payload: {
          stripe_customer_id: customer.id,
          value: "1", // 1 request
        },
      });

      usageEvents.push(event);
      console.log(`   ✓ Event ${i}: ${(event as any).id} recorded`);

      // Small delay to simulate real usage over time
      await new Promise((resolve) => setTimeout(resolve, 100));
    }

    console.log(`\n   ✓ Total events recorded: ${usageEvents.length}\n`);

    // Step 4: Summary
    console.log("✅ Usage-based billing events recorded!\n");

    console.log("📋 Usage Summary:");
    console.log(`   Customer: ${customer.name} (${customer.email})`);
    console.log(`   Meter: ${rpcMeter.display_name}`);
    console.log(`   Event Type: ${rpcMeter.event_name}`);
    console.log(`   Total RPC Requests: ${usageEvents.length}`);
    console.log(`\n💡 These events would be aggregated and billed based on`);
    console.log(
      `   the meter configuration at the end of the billing period.\n`,
    );

    return {
      customer,
      meter: rpcMeter,
      events: usageEvents,
    };
  } catch (error) {
    if (error instanceof Error) {
      console.error("❌ Error:", error.message);
    }
    throw error;
  }
}

/**
 * Combined example: Subscription + Usage-based billing
 */
async function combinedBillingExample() {
  console.log("🚀 Combined Billing Example: Subscription + Usage\n");
  console.log("This demonstrates a hybrid model where customers pay a base");
  console.log("subscription fee plus usage-based charges for API requests.\n");

  const stripe = await initializeClient();

  try {
    // Step 1: Create subscription
    console.log("=".repeat(60));
    console.log("PART 1: Setting up Base Subscription");
    console.log("=".repeat(60) + "\n");

    const { customer, subscription } = await purchaseSubscription();

    console.log("\n" + "=".repeat(60));
    console.log("PART 2: Tracking Usage-Based Billing");
    console.log("=".repeat(60) + "\n");

    // Step 2: Get meter
    console.log("1️⃣ Fetching billing meter...");
    const meters = await stripe.billing.meters.list({ limit: 10 });
    const rpcMeter = meters.data.find(
      (m) => m.event_name === "surfnet_rpc_requests",
    );

    if (!rpcMeter) {
      throw new Error("Surfnet RPC requests meter not found");
    }

    console.log(`   ✓ Found: ${rpcMeter.display_name}`);
    console.log(`   ✓ Event: ${rpcMeter.event_name}\n`);

    // Step 3: Record usage for the subscribed customer
    console.log("2️⃣ Recording API usage for subscribed customer...");
    const usageEvents = [];

    // Simulate 10 RPC requests
    for (let i = 1; i <= 10; i++) {
      const event = await stripe.billing.meterEvents.create({
        event_name: "surfnet_rpc_requests",
        payload: {
          stripe_customer_id: customer.id,
          value: "1",
        },
      });

      usageEvents.push(event);
      console.log(`   ✓ RPC Request ${i}: ${(event as any).id}`);

      await new Promise((resolve) => setTimeout(resolve, 100));
    }

    console.log(`\n   ✓ Total API calls recorded: ${usageEvents.length}\n`);

    // Summary
    console.log("=".repeat(60));
    console.log("BILLING SUMMARY");
    console.log("=".repeat(60) + "\n");

    console.log("📋 Monthly Charges:");
    console.log(`   Base Subscription: $499.00/month (Surfpool Max)`);
    console.log(
      `   Usage Charges: ${usageEvents.length} RPC requests @ $0.10 each = $${(usageEvents.length * 0.1).toFixed(2)}`,
    );
    console.log(`   ─────────────────────────────────────────────`);
    console.log(
      `   Total Estimated Bill: $${(499 + usageEvents.length * 0.1).toFixed(2)}\n`,
    );

    console.log("💡 Billing Flow:");
    console.log(`   • Customer pays $499/month base subscription`);
    console.log(`   • Each API request is metered via billing.meterEvents`);
    console.log(`   • At billing period end, usage is aggregated`);
    console.log(`   • Combined invoice includes base + usage charges\n`);

    return {
      customer,
      subscription,
      meter: rpcMeter,
      usageEvents,
    };
  } catch (error) {
    if (error instanceof Error) {
      console.error("❌ Error:", error.message);
    }
    throw error;
  }
}

async function main() {
  // Uncomment to list catalog
  // await listCatalog();

  // Run the subscription purchase flow
  // await purchaseSubscription();

  // Run the usage-based billing example
  // await usageBasedBilling();

  // Run the combined example (subscription + usage)
  await combinedBillingExample();
}

main();
