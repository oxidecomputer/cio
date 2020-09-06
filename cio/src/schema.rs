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
    conference_rooms (id) {
        id -> Int4,
        name -> Varchar,
        description -> Varchar,
        typev -> Varchar,
        building -> Varchar,
        capacity -> Int4,
        floor -> Varchar,
        section -> Varchar,
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
    groups (id) {
        id -> Int4,
        name -> Varchar,
        description -> Varchar,
        aliases -> Array<Text>,
        allow_external_members -> Bool,
        allow_web_posting -> Bool,
        is_archived -> Bool,
        who_can_discover_group -> Varchar,
        who_can_join -> Varchar,
        who_can_moderate_members -> Varchar,
        who_can_post_message -> Varchar,
        who_can_view_group -> Varchar,
        who_can_view_membership -> Varchar,
    }
}

table! {
    links (id) {
        id -> Int4,
        name -> Varchar,
        description -> Varchar,
        link -> Varchar,
        aliases -> Array<Text>,
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
    conference_rooms,
    github_labels,
    groups,
    links,
    users,
);
