pub mod billing;
pub mod checkout_sessions;
pub mod common;
pub mod customers;
pub mod payment_intents;
pub mod payment_methods;
pub mod prices;
pub mod products;
pub mod subscriptions;

// Re-export common catalog types from moneymq-types
// Re-export types specific to moneymq-core (not in moneymq-types)
pub use billing::StripeMeterEvent;
pub use checkout_sessions::{
    CheckoutLineItem, CheckoutLineItemList, CheckoutLineItemPrice, CheckoutSessionStatus,
    CreateCheckoutSessionRequest, CreateLineItem, PaymentStatus, StripeCheckoutSession,
};
pub use customers::{CreateCustomerRequest, StripeCustomer};
pub use moneymq_types::stripe::{
    ListParams, ListResponse, StripeBillingMeter, StripePrice, StripeRecurring,
};
// Use local enhanced StripeProduct with experiment support
pub use payment_intents::{
    ConfirmPaymentIntentRequest, CreatePaymentIntentRequest, PaymentIntentStatus,
    StripePaymentIntent,
};
pub use payment_methods::{
    AttachPaymentMethodRequest, CreatePaymentMethodRequest, StripeCard, StripePaymentMethod,
};
pub use products::{StripeExperimentConfig, StripeProduct};
pub use subscriptions::{
    StripeSubscription, SubscriptionItemData, SubscriptionItems, SubscriptionPrice,
};
