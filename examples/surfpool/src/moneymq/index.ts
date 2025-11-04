export { payUpon402 } from './payUpon402';
export { createStripeClient } from './createStripeClient';

// Re-export x402-fetch utilities for convenience
export { createSigner } from 'x402-fetch';
export { createPaymentHeader, selectPaymentRequirements } from 'x402/client';
export type { PaymentRequirements, Signer, MultiNetworkSigner } from 'x402/types';
