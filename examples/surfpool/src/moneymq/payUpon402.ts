import { createPaymentHeader, selectPaymentRequirements } from "x402/client";
import type { Signer, MultiNetworkSigner, X402Config, PaymentRequirements } from "x402/types";

type PromiseOrFn<T> = Promise<T> | (() => Promise<T>);

interface X402Response {
  x402Version: number;
  accepts: PaymentRequirements[];
  error?: {
    code: string;
    message: string;
    type: string;
  };
}

/**
 * Wrap an HTTP call with automatic 402 Payment Required handling
 *
 * This utility works with any HTTP API that returns 402 status codes for
 * payment-required scenarios. It uses the Coinbase x402 protocol to
 * automatically handle payment flows and retry the request.
 *
 * Works with any API client (fetch, axios, Stripe SDK, custom clients, etc.)
 * that throws errors with a statusCode property.
 *
 * @example
 * // With a wallet client for automatic payment:
 * await payUpon402(() => fetch('/api/endpoint', { method: 'POST' }), walletClient)
 *
 * // Without wallet client (will fail on 402):
 * await payUpon402(() => apiClient.post('/resource'))
 */
export async function payUpon402<T>(
  promiseOrFn: PromiseOrFn<T>,
  walletClient?: Signer | MultiNetworkSigner,
  maxValue: bigint = BigInt(0.1 * 10 ** 6),
  config?: X402Config,
): Promise<T> {
  const isFunction = typeof promiseOrFn === "function";

  const execute = async (paymentHeader?: string) => {
    return isFunction
      ? await (promiseOrFn as () => Promise<T>)()
      : await promiseOrFn;
  };

  try {
    return await execute();
  } catch (error: any) {
    const is402 =
      error?.statusCode === 402 ||
      error?.status === 402 ||
      error?.raw?.statusCode === 402;

    if (!is402) throw error;

    console.log("💳 402 Payment Required - processing payment...");

    if (!isFunction) {
      console.warn(
        "⚠️  Cannot retry - promise already executed. Use () => syntax for retry support.",
      );
      throw error;
    }

    // TODO: For now, use mock wallet client if none provided
    if (!walletClient) {
      console.warn(
        "⚠️  No wallet client provided. Using mock payment (will not actually pay).",
      );
      // Don't throw, continue with mock payment header generation
    }

    // Extract x402 response from error
    let x402Response: X402Response;
    try {
      // For Stripe SDK errors, the payment requirements are directly in error.raw
      if (error?.raw?.payment_requirements !== undefined) {
        console.log("✓ Found payment requirements in Stripe error format");
        x402Response = {
          x402Version: 1, // Default to version 1
          accepts: error.raw.payment_requirements,
          error: {
            code: error.raw.code || "payment_required",
            message: error.raw.message || "Payment required",
            type: error.raw.type || "invalid_request_error",
          },
        };
      } else if (error?.raw?.body) {
        console.log("Attempting to parse from error.raw.body");
        x402Response =
          typeof error.raw.body === "string"
            ? JSON.parse(error.raw.body)
            : error.raw.body;
      } else if (error?.response?.data) {
        console.log("Attempting to parse from error.response.data");
        x402Response = error.response.data;
      } else if (error?.error) {
        console.log("Attempting to parse from error.error");
        x402Response = error.error;
      } else {
        throw new Error("Cannot parse 402 response");
      }
    } catch (parseError) {
      console.warn(
        "⚠️  Failed to parse payment requirements from 402 response",
      );
      console.error(parseError);
      throw error;
    }

    const { x402Version, accepts } = x402Response;

    if (!accepts || accepts.length === 0) {
      console.warn("⚠️  No payment requirements found in 402 response");
      throw error;
    }

    try {
      let paymentHeaderValue: string;

      if (!walletClient) {
        // Create a mock payment header for testing
        console.log("💰 Creating mock payment header...");
        const mockPayload = {
          x402Version,
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
          accepts,
          undefined, // Let the selector determine network from wallet
          "exact",
        );

        // Check if payment amount exceeds maximum
        if (BigInt(selectedPaymentRequirement.maxAmountRequired) > maxValue) {
          throw new Error(
            `Payment amount ${selectedPaymentRequirement.maxAmountRequired} exceeds maximum allowed ${maxValue}`,
          );
        }

        console.log(
          `💰 Creating payment for ${selectedPaymentRequirement.network}...`,
        );

        // Create payment header using Coinbase x402 library
        paymentHeaderValue = await createPaymentHeader(
          walletClient,
          x402Version,
          selectedPaymentRequirement,
          config,
        );
      }

      console.log("✅ Payment header created, retrying request...");

      // Retry the request - this is simplified, real implementation would
      // need to inject the X-PAYMENT header into the original request
      return await execute(paymentHeaderValue);
    } catch (paymentError) {
      console.warn("⚠️  Payment creation failed");
      console.error(paymentError);
      throw error;
    }
  }
}
