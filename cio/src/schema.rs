table! {
    buildings (id) {
        id -> Int4,
        name -> Varchar,
        description -> Varchar,
        address -> Varchar,
        city -> Varchar,
        state -> Varchar,
        zipcode -> Varchar,
        country -> Varchar,
        floors -> Array<Text>,
    }
}

table! {
    github_labels (id) {
        id -> Int4,
        name -> Varchar,
        description -> Varchar,
        color -> Varchar,
    }
}

table! {
    users (id) {
        id -> Int4,
        first_name -> Varchar,
        last_name -> Varchar,
        username -> Varchar,
        aliases -> Array<Text>,
        recovery_email -> Varchar,
        recovery_phone -> Varchar,
        gender -> Varchar,
        chat -> Varchar,
        github -> Varchar,
        twitter -> Varchar,
        groups -> Array<Text>,
        is_super_admin -> Bool,
        building -> Varchar,
    }
}

allow_tables_to_appear_in_same_query!(
    buildings,
    github_labels,
    users,
);
