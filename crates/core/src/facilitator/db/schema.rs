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

diesel::joinable!(facilitated_transactions -> transaction_customers (customer_id));

diesel::allow_tables_to_appear_in_same_query!(facilitated_transactions, transaction_customers,);
