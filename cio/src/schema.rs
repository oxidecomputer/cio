table! {
    applicants (id) {
        id -> Int4,
        name -> Varchar,
        role -> Varchar,
        sheet_id -> Varchar,
        status -> Varchar,
        submitted_time -> Timestamptz,
        email -> Varchar,
        phone -> Varchar,
        country_code -> Varchar,
        location -> Varchar,
        github -> Varchar,
        gitlab -> Varchar,
        linkedin -> Varchar,
        portfolio -> Varchar,
        website -> Varchar,
        resume -> Varchar,
        materials -> Varchar,
        sent_email_received -> Bool,
        value_reflected -> Varchar,
        value_violated -> Varchar,
        values_in_tension -> Array<Text>,
        resume_contents -> Text,
        materials_contents -> Text,
        work_samples -> Text,
        writing_samples -> Text,
        analysis_samples -> Text,
        presentation_samples -> Text,
        exploratory_samples -> Text,
        question_technically_challenging -> Text,
        question_proud_of -> Text,
        question_happiest -> Text,
        question_unhappiest -> Text,
        question_value_reflected -> Text,
        question_value_violated -> Text,
        question_values_in_tension -> Text,
        question_why_oxide -> Text,
    }
}

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
    mailing_list_subscribers (id) {
        id -> Int4,
        email -> Varchar,
        first_name -> Varchar,
        last_name -> Varchar,
        name -> Varchar,
        company -> Varchar,
        interest -> Text,
        wants_podcast_updates -> Bool,
        wants_newsletter -> Bool,
        wants_product_updates -> Bool,
        date_added -> Timestamptz,
        date_optin -> Timestamptz,
        date_last_changed -> Timestamptz,
        notes -> Text,
        tags -> Array<Text>,
        link_to_people -> Array<Text>,
    }
}

table! {
    rfds (id) {
        id -> Int4,
        number -> Int4,
        number_string -> Varchar,
        title -> Varchar,
        name -> Varchar,
        state -> Varchar,
        link -> Varchar,
        short_link -> Varchar,
        rendered_link -> Varchar,
        discussion -> Varchar,
        authors -> Varchar,
        html -> Text,
        content -> Text,
        sha -> Varchar,
        commit_date -> Timestamptz,
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
    applicants,
    buildings,
    conference_rooms,
    github_labels,
    groups,
    links,
    mailing_list_subscribers,
    rfds,
    users,
);
