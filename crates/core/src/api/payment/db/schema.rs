diesel::table! {
    facilitated_transactions (id) {
        id -> Int4,
        created_at -> Int8,
        updated_at -> Int8,
        product -> Nullable<Text>,
        customer_id -> Nullable<Int4>,
        amount -> Text,
        currency -> Nullable<Text>,
        status -> Nullable<Text>,
        signature -> Nullable<Text>,
        x402_payment_requirement -> Text,
        x402_verify_request -> Nullable<Text>,
        x402_verify_response -> Nullable<Text>,
        x402_settle_request -> Nullable<Text>,
        x402_settle_response -> Nullable<Text>,
        payment_hash -> Nullable<Text>,
        payment_stack_id -> Text,
        is_sandbox -> Bool,
    }
}

diesel::table! {
    transaction_customers (id) {
        id -> Int4,
        created_at -> Int8,
        updated_at -> Int8,
        label -> Nullable<Text>,
        address -> Text,
    }
}

diesel::table! {
    cloud_events (id) {
        id -> Int4,
        event_id -> Text,
        event_type -> Text,
        event_source -> Text,
        event_time -> Int8,
        data_json -> Text,
        payment_stack_id -> Text,
        is_sandbox -> Bool,
        created_at -> Int8,
    }
}

diesel::table! {
    event_streams (id) {
        id -> Int4,
        stream_id -> Text,
        payment_stack_id -> Text,
        is_sandbox -> Bool,
        last_event_id -> Nullable<Text>,
        last_event_time -> Nullable<Int8>,
        created_at -> Int8,
        updated_at -> Int8,
    }
}

diesel::joinable!(facilitated_transactions -> transaction_customers (customer_id));

diesel::allow_tables_to_appear_in_same_query!(
    facilitated_transactions,
    transaction_customers,
    cloud_events,
    event_streams,
);
