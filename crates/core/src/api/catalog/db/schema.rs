diesel::table! {
    products (id) {
        id -> Int4,
        created_at -> Int8,
        updated_at -> Int8,
        payment_stack_id -> Text,
        product_id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        product_type -> Text,
        unit_label -> Nullable<Text>,
        active -> Bool,
        metadata -> Nullable<Text>,
        is_sandbox -> Bool,
    }
}

diesel::table! {
    prices (id) {
        id -> Int4,
        created_at -> Int8,
        updated_at -> Int8,
        product_id -> Int4,
        price_id -> Nullable<Text>,
        pricing_type -> Text,
        currency -> Text,
        unit_amount -> Int8,
        recurring_interval -> Nullable<Text>,
        recurring_interval_count -> Nullable<Int4>,
        active -> Bool,
        metadata -> Nullable<Text>,
    }
}

diesel::joinable!(prices -> products (product_id));

diesel::allow_tables_to_appear_in_same_query!(products, prices,);
