pub mod billing;
pub mod checkout_sessions;
pub mod customers;
pub mod payment_intents;
pub mod payment_methods;
pub mod prices;
pub mod products;
pub mod subscriptions;

// Re-export handlers for convenience
pub use billing::{create_meter_event, list_meters};
pub use checkout_sessions::{
    create_checkout_session, expire_checkout_session, list_checkout_session_line_items,
    retrieve_checkout_session,
};
pub use customers::{create_customer, update_customer};
pub use payment_intents::{
    cancel_payment_intent, confirm_payment_intent, create_payment_intent, retrieve_payment_intent,
};
pub use payment_methods::{attach_payment_method, create_payment_method};
pub use prices::list_prices;
pub use products::{get_product_access, list_products};
pub use subscriptions::create_subscription;
