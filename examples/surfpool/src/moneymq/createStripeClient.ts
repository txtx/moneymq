import Stripe from "stripe";
import { createPaymentHeader, selectPaymentRequirements } from "x402/client";
import type {
  Signer,
  MultiNetworkSigner,
  X402Config,
  PaymentRequirements,
} from "x402/types";

/**
 * Creates a Stripe client with automatic X402 payment handling
 *
 * @param apiKey - Stripe API key
 * @param walletClient - Optional wallet client for creating payments
 * @param config - Stripe configuration options
 * @param x402Config - Optional X402 configuration
 * @returns A Stripe client instance with payment middleware
 */
export function createStripeClient(
  apiKey: string,
  walletClient?: Signer | MultiNetworkSigner,
  config?: Stripe.StripeConfig,
  x402Config?: X402Config,
): Stripe {
  // Create a custom HTTP client that wraps fetch
  const customHttpClient = Stripe.createFetchHttpClient();

  // Wrap the makeRequest method to add X-Payment header on retry
  const originalMakeRequest = customHttpClient.makeRequest.bind(
    customHttpClient,
  );

  customHttpClient.makeRequest = async (
    host: string,
    port: string | number,
    path: string,
    method: "GET" | "POST" | "PUT" | "DELETE",
    headers: object,
    requestData: string | null,
    protocol: Stripe.HttpProtocol,
    timeout: number,
  ) => {
    console.log(`🔍 Making request to ${method} ${host}:${port}${path}`);

    // Make the initial request
    const response = await originalMakeRequest(
      host,
      port,
      path,
      method,
      headers,
      requestData,
      protocol,
      timeout,
    );

    const statusCode = response.getStatusCode();
    console.log(`📥 Response status: ${statusCode}`);

    // Check if this is a 402 Payment Required response
    if (statusCode === 402) {
      console.log("💳 402 Payment Required - processing payment...");

      // Parse the response body to get payment requirements
      const responseBody = (await response.toJSON()) as any;
      console.log("📄 Response body:", JSON.stringify(responseBody, null, 2));

      const paymentRequirements: PaymentRequirements[] =
        responseBody?.payment_requirements ||
        responseBody?.error?.payment_requirements ||
        [];

      if (paymentRequirements.length === 0) {
        console.warn("⚠️  No payment requirements found in 402 response");
        return response;
      }

      let paymentHeaderValue: string;

      if (!walletClient) {
        // Create mock payment header
        console.warn(
          "⚠️  No wallet client provided. Using mock payment (will not actually pay).",
        );
        const mockPayload = {
          x402Version: 1,
          scheme: "exact",
          network: "solana",
          payload: {
            transaction: "mock_base58_encoded_transaction",
          },
        };
        paymentHeaderValue = Buffer.from(JSON.stringify(mockPayload)).toString(
          "base64",
        );
      } else {

        // Select appropriate payment requirement
        const selectedPaymentRequirement = selectPaymentRequirements(
          paymentRequirements,
          undefined,
          "exact",
        );

        console.log(
          `💰 Creating payment for ${selectedPaymentRequirement.network}...`,
        );


        // Extract the appropriate signer for the network
        let signer: Signer;
        if ("svm" in walletClient) {
          // MultiNetworkSigner - extract the svm signer
          signer = walletClient.svm;
        } else {
          // Already a Signer
          signer = walletClient;
        }

        // Create payment header using Coinbase x402 library
        // Add svmConfig with local RPC URL for local testing
        const effectiveX402Config = {
          ...x402Config,
          svmConfig: {
            rpcUrl: "http://localhost:8899",
          },
        };
        console.log(effectiveX402Config);

        paymentHeaderValue = await createPaymentHeader(
          signer,
          1, // x402Version
          selectedPaymentRequirement,
          effectiveX402Config,
        );
      }

      console.log("✅ Payment header created, retrying request...");

      // Retry with X-Payment header
      const headersWithPayment = {
        ...(headers as Record<string, string>),
        "X-Payment": paymentHeaderValue,
      };

      console.log("📤 Retrying with X-Payment header");

      return await originalMakeRequest(
        host,
        port,
        path,
        method,
        headersWithPayment,
        requestData,
        protocol,
        timeout,
      );
    }

    return response;
  };

  return new Stripe(apiKey, {
    ...config,
    httpClient: customHttpClient,
    maxNetworkRetries: 0, // Disable Stripe's automatic retries so we can handle 402
  });
}
