table! {
    accounts_payables (id) {
        id -> Int4,
        confirmation_number -> Varchar,
        amount -> Float4,
        invoice_number -> Varchar,
        vendor -> Varchar,
        currency -> Varchar,
        date -> Date,
        payment_type -> Varchar,
        status -> Varchar,
        link_to_vendor -> Array<Text>,
        airtable_record_id -> Varchar,
    }
}

table! {
    applicant_interviews (id) {
        id -> Int4,
        start_time -> Timestamptz,
        end_time -> Timestamptz,
        name -> Varchar,
        email -> Varchar,
        interviewers -> Array<Text>,
        google_event_id -> Varchar,
        event_link -> Varchar,
        applicant -> Array<Text>,
        airtable_record_id -> Varchar,
    }
}

table! {
    applicant_reviewers (id) {
        id -> Int4,
        name -> Varchar,
        email -> Varchar,
        evaluations -> Int4,
        emphatic_yes -> Int4,
        yes -> Int4,
        pass -> Int4,
        no -> Int4,
        not_applicable -> Int4,
        airtable_record_id -> Varchar,
    }
}

table! {
    applicants (id) {
        id -> Int4,
        name -> Varchar,
        role -> Varchar,
        sheet_id -> Varchar,
        status -> Varchar,
        raw_status -> Varchar,
        submitted_time -> Timestamptz,
        email -> Varchar,
        phone -> Varchar,
        country_code -> Varchar,
        location -> Varchar,
        latitude -> Float4,
        longitude -> Float4,
        github -> Varchar,
        gitlab -> Varchar,
        linkedin -> Varchar,
        portfolio -> Varchar,
        website -> Varchar,
        resume -> Varchar,
        materials -> Varchar,
        sent_email_received -> Bool,
        sent_email_follow_up -> Bool,
        rejection_sent_date_time -> Nullable<Timestamptz>,
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
        interview_packet -> Varchar,
        interviews -> Array<Text>,
        interviews_started -> Nullable<Timestamptz>,
        interviews_completed -> Nullable<Timestamptz>,
        scorers -> Array<Text>,
        scorers_completed -> Array<Text>,
        scoring_form_id -> Varchar,
        scoring_form_url -> Varchar,
        scoring_form_responses_url -> Varchar,
        scoring_evaluations_count -> Int4,
        scoring_enthusiastic_yes_count -> Int4,
        scoring_yes_count -> Int4,
        scoring_pass_count -> Int4,
        scoring_no_count -> Int4,
        scoring_not_applicable_count -> Int4,
        scoring_insufficient_experience_count -> Int4,
        scoring_inapplicable_experience_count -> Int4,
        scoring_job_function_yet_needed_count -> Int4,
        scoring_underwhelming_materials_count -> Int4,
        criminal_background_check_status -> Varchar,
        motor_vehicle_background_check_status -> Varchar,
        start_date -> Nullable<Date>,
        interested_in -> Array<Text>,
        geocode_cache -> Varchar,
        docusign_envelope_id -> Varchar,
        docusign_envelope_status -> Varchar,
        offer_created -> Nullable<Timestamptz>,
        offer_completed -> Nullable<Timestamptz>,
        airtable_record_id -> Varchar,
    }
}

table! {
    auth_user_logins (id) {
        id -> Int4,
        date -> Timestamptz,
        typev -> Varchar,
        description -> Varchar,
        connection -> Varchar,
        connection_id -> Varchar,
        client_id -> Varchar,
        client_name -> Varchar,
        ip -> Varchar,
        hostname -> Varchar,
        user_id -> Varchar,
        user_name -> Varchar,
        email -> Varchar,
        audience -> Varchar,
        scope -> Varchar,
        strategy -> Varchar,
        strategy_type -> Varchar,
        log_id -> Varchar,
        is_mobile -> Bool,
        user_agent -> Varchar,
        link_to_auth_user -> Array<Text>,
        airtable_record_id -> Varchar,
    }
}

table! {
    auth_users (id) {
        id -> Int4,
        user_id -> Varchar,
        name -> Varchar,
        nickname -> Varchar,
        username -> Varchar,
        email -> Varchar,
        email_verified -> Bool,
        picture -> Varchar,
        company -> Varchar,
        blog -> Varchar,
        phone -> Varchar,
        phone_verified -> Bool,
        locale -> Varchar,
        login_provider -> Varchar,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        last_login -> Timestamptz,
        last_application_accessed -> Varchar,
        last_ip -> Varchar,
        logins_count -> Int4,
        link_to_people -> Array<Text>,
        link_to_auth_user_logins -> Array<Text>,
        link_to_page_views -> Array<Text>,
        airtable_record_id -> Varchar,
    }
}

table! {
    barcode_scans (id) {
        id -> Int4,
        time -> Timestamptz,
        name -> Varchar,
        size -> Varchar,
        item -> Varchar,
        barcode -> Varchar,
        link_to_item -> Array<Text>,
        airtable_record_id -> Varchar,
    }
}

table! {
    buildings (id) {
        id -> Int4,
        name -> Varchar,
        description -> Varchar,
        street_address -> Varchar,
        city -> Varchar,
        state -> Varchar,
        zipcode -> Varchar,
        country -> Varchar,
        address_formatted -> Varchar,
        floors -> Array<Text>,
        employees -> Array<Text>,
        conference_rooms -> Array<Text>,
        geocode_cache -> Varchar,
        airtable_record_id -> Varchar,
    }
}

table! {
    certificates (id) {
        id -> Int4,
        domain -> Varchar,
        certificate -> Text,
        private_key -> Text,
        valid_days_left -> Int4,
        expiration_date -> Date,
        airtable_record_id -> Varchar,
    }
}

table! {
    conference_rooms (id) {
        id -> Int4,
        name -> Varchar,
        description -> Varchar,
        typev -> Varchar,
        building -> Varchar,
        link_to_building -> Array<Text>,
        capacity -> Int4,
        floor -> Varchar,
        section -> Varchar,
        airtable_record_id -> Varchar,
    }
}

table! {
    credit_card_transactions (id) {
        id -> Int4,
        transaction_id -> Varchar,
        card_vendor -> Varchar,
        amount -> Float4,
        employee_email -> Varchar,
        card_id -> Varchar,
        merchant_id -> Varchar,
        merchant_name -> Varchar,
        category_id -> Int4,
        category_name -> Varchar,
        state -> Varchar,
        memo -> Varchar,
        time -> Timestamptz,
        receipts -> Array<Text>,
        link_to_vendor -> Array<Text>,
        airtable_record_id -> Varchar,
    }
}

table! {
    expensed_items (id) {
        id -> Int4,
        transaction_id -> Varchar,
        expenses_vendor -> Varchar,
        amount -> Float4,
        employee_email -> Varchar,
        card_id -> Varchar,
        merchant_id -> Varchar,
        merchant_name -> Varchar,
        category_id -> Int4,
        category_name -> Varchar,
        state -> Varchar,
        memo -> Varchar,
        time -> Timestamptz,
        receipts -> Array<Text>,
        link_to_vendor -> Array<Text>,
        airtable_record_id -> Varchar,
    }
}

table! {
    github_repos (id) {
        id -> Int4,
        github_id -> Varchar,
        owner -> Varchar,
        name -> Varchar,
        full_name -> Varchar,
        description -> Varchar,
        private -> Bool,
        fork -> Bool,
        url -> Varchar,
        html_url -> Varchar,
        archive_url -> Varchar,
        assignees_url -> Varchar,
        blobs_url -> Varchar,
        branches_url -> Varchar,
        clone_url -> Varchar,
        collaborators_url -> Varchar,
        comments_url -> Varchar,
        commits_url -> Varchar,
        compare_url -> Varchar,
        contents_url -> Varchar,
        contributors_url -> Varchar,
        deployments_url -> Varchar,
        downloads_url -> Varchar,
        events_url -> Varchar,
        forks_url -> Varchar,
        git_commits_url -> Varchar,
        git_refs_url -> Varchar,
        git_tags_url -> Varchar,
        git_url -> Varchar,
        hooks_url -> Varchar,
        issue_comment_url -> Varchar,
        issue_events_url -> Varchar,
        issues_url -> Varchar,
        keys_url -> Varchar,
        labels_url -> Varchar,
        languages_url -> Varchar,
        merges_url -> Varchar,
        milestones_url -> Varchar,
        mirror_url -> Varchar,
        notifications_url -> Varchar,
        pulls_url -> Varchar,
        releases_url -> Varchar,
        ssh_url -> Varchar,
        stargazers_url -> Varchar,
        statuses_url -> Varchar,
        subscribers_url -> Varchar,
        subscription_url -> Varchar,
        svn_url -> Varchar,
        tags_url -> Varchar,
        teams_url -> Varchar,
        trees_url -> Varchar,
        homepage -> Varchar,
        language -> Varchar,
        forks_count -> Int4,
        stargazers_count -> Int4,
        watchers_count -> Int4,
        size -> Int4,
        default_branch -> Varchar,
        open_issues_count -> Int4,
        has_issues -> Bool,
        has_wiki -> Bool,
        has_pages -> Bool,
        has_downloads -> Bool,
        archived -> Bool,
        pushed_at -> Timestamptz,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        airtable_record_id -> Varchar,
    }
}

table! {
    groups (id) {
        id -> Int4,
        name -> Varchar,
        description -> Varchar,
        link -> Varchar,
        aliases -> Array<Text>,
        members -> Array<Text>,
        allow_external_members -> Bool,
        allow_web_posting -> Bool,
        is_archived -> Bool,
        who_can_discover_group -> Varchar,
        who_can_join -> Varchar,
        who_can_moderate_members -> Varchar,
        who_can_post_message -> Varchar,
        who_can_view_group -> Varchar,
        who_can_view_membership -> Varchar,
        enable_collaborative_inbox -> Bool,
        airtable_record_id -> Varchar,
    }
}

table! {
    inbound_shipments (id) {
        id -> Int4,
        tracking_number -> Varchar,
        carrier -> Varchar,
        tracking_link -> Varchar,
        oxide_tracking_link -> Varchar,
        tracking_status -> Varchar,
        shipped_time -> Nullable<Timestamptz>,
        delivered_time -> Nullable<Timestamptz>,
        eta -> Nullable<Timestamptz>,
        messages -> Varchar,
        name -> Varchar,
        notes -> Varchar,
        airtable_record_id -> Varchar,
    }
}

table! {
    journal_club_meetings (id) {
        id -> Int4,
        title -> Varchar,
        issue -> Varchar,
        papers -> Array<Text>,
        issue_date -> Date,
        meeting_date -> Date,
        coordinator -> Varchar,
        state -> Varchar,
        recording -> Varchar,
        airtable_record_id -> Varchar,
    }
}

table! {
    journal_club_papers (id) {
        id -> Int4,
        title -> Varchar,
        link -> Varchar,
        meeting -> Varchar,
        link_to_meeting -> Array<Text>,
        airtable_record_id -> Varchar,
    }
}

table! {
    links (id) {
        id -> Int4,
        name -> Varchar,
        description -> Varchar,
        link -> Varchar,
        aliases -> Array<Text>,
        short_link -> Varchar,
        airtable_record_id -> Varchar,
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
        airtable_record_id -> Varchar,
    }
}

table! {
    outbound_shipments (id) {
        id -> Int4,
        name -> Varchar,
        contents -> Varchar,
        street_1 -> Varchar,
        street_2 -> Varchar,
        city -> Varchar,
        state -> Varchar,
        zipcode -> Varchar,
        country -> Varchar,
        address_formatted -> Varchar,
        latitude -> Float4,
        longitude -> Float4,
        email -> Varchar,
        phone -> Varchar,
        status -> Varchar,
        carrier -> Varchar,
        tracking_number -> Varchar,
        tracking_link -> Varchar,
        oxide_tracking_link -> Varchar,
        tracking_status -> Varchar,
        label_link -> Varchar,
        cost -> Float4,
        pickup_date -> Nullable<Date>,
        created_time -> Timestamptz,
        shipped_time -> Nullable<Timestamptz>,
        delivered_time -> Nullable<Timestamptz>,
        eta -> Nullable<Timestamptz>,
        shippo_id -> Varchar,
        messages -> Varchar,
        notes -> Varchar,
        geocode_cache -> Varchar,
        local_pickup -> Bool,
        airtable_record_id -> Varchar,
    }
}

table! {
    page_views (id) {
        id -> Int4,
        time -> Timestamptz,
        domain -> Varchar,
        path -> Varchar,
        user_email -> Varchar,
        page_link -> Varchar,
        link_to_auth_user -> Array<Text>,
        airtable_record_id -> Varchar,
    }
}

table! {
    rack_line_subscribers (id) {
        id -> Int4,
        email -> Varchar,
        name -> Varchar,
        company -> Varchar,
        company_size -> Varchar,
        interest -> Text,
        date_added -> Timestamptz,
        date_optin -> Timestamptz,
        date_last_changed -> Timestamptz,
        notes -> Text,
        tags -> Array<Text>,
        link_to_people -> Array<Text>,
        airtable_record_id -> Varchar,
    }
}

table! {
    recorded_meetings (id) {
        id -> Int4,
        name -> Varchar,
        description -> Varchar,
        start_time -> Timestamptz,
        end_time -> Timestamptz,
        video -> Varchar,
        chat_log_link -> Varchar,
        chat_log -> Varchar,
        is_recurring -> Bool,
        attendees -> Array<Text>,
        transcript -> Text,
        transcript_id -> Varchar,
        google_event_id -> Varchar,
        event_link -> Varchar,
        location -> Varchar,
        airtable_record_id -> Varchar,
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
        milestones -> Array<Text>,
        relevant_components -> Array<Text>,
        pdf_link_github -> Varchar,
        pdf_link_google_drive -> Varchar,
        airtable_record_id -> Varchar,
    }
}

table! {
    software_vendors (id) {
        id -> Int4,
        name -> Varchar,
        status -> Varchar,
        description -> Varchar,
        category -> Varchar,
        website -> Varchar,
        has_okta_integration -> Bool,
        used_purely_for_api -> Bool,
        pay_as_you_go -> Bool,
        pay_as_you_go_pricing_description -> Varchar,
        software_licenses -> Bool,
        cost_per_user_per_month -> Float4,
        users -> Int4,
        flat_cost_per_month -> Float4,
        total_cost_per_month -> Float4,
        groups -> Array<Text>,
        link_to_transactions -> Array<Text>,
        link_to_accounts_payable -> Array<Text>,
        link_to_expensed_items -> Array<Text>,
        airtable_record_id -> Varchar,
    }
}

table! {
    swag_inventory_items (id) {
        id -> Int4,
        name -> Varchar,
        size -> Varchar,
        current_stock -> Int4,
        item -> Varchar,
        barcode -> Varchar,
        barcode_png -> Varchar,
        barcode_svg -> Varchar,
        barcode_pdf_label -> Varchar,
        print_barcode_label_quantity -> Int4,
        link_to_item -> Array<Text>,
        airtable_record_id -> Varchar,
    }
}

table! {
    swag_items (id) {
        id -> Int4,
        name -> Varchar,
        description -> Varchar,
        image -> Varchar,
        internal_only -> Bool,
        link_to_inventory -> Array<Text>,
        link_to_barcode_scans -> Array<Text>,
        link_to_order_january_2020 -> Array<Text>,
        link_to_order_october_2020 -> Array<Text>,
        link_to_order_may_2021 -> Array<Text>,
        airtable_record_id -> Varchar,
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
        department -> Varchar,
        manager -> Varchar,
        link_to_manager -> Array<Text>,
        groups -> Array<Text>,
        is_group_admin -> Bool,
        building -> Varchar,
        link_to_building -> Array<Text>,
        aws_role -> Varchar,
        home_address_street_1 -> Varchar,
        home_address_street_2 -> Varchar,
        home_address_city -> Varchar,
        home_address_state -> Varchar,
        home_address_zipcode -> Varchar,
        home_address_country -> Varchar,
        home_address_country_code -> Varchar,
        home_address_formatted -> Varchar,
        home_address_latitude -> Float4,
        home_address_longitude -> Float4,
        work_address_street_1 -> Varchar,
        work_address_street_2 -> Varchar,
        work_address_city -> Varchar,
        work_address_state -> Varchar,
        work_address_zipcode -> Varchar,
        work_address_country -> Varchar,
        work_address_country_code -> Varchar,
        work_address_formatted -> Varchar,
        start_date -> Date,
        birthday -> Date,
        public_ssh_keys -> Array<Text>,
        typev -> Varchar,
        google_anniversary_event_id -> Varchar,
        geocode_cache -> Varchar,
        airtable_record_id -> Varchar,
    }
}

allow_tables_to_appear_in_same_query!(
    accounts_payables,
    applicant_interviews,
    applicant_reviewers,
    applicants,
    auth_user_logins,
    auth_users,
    barcode_scans,
    buildings,
    certificates,
    conference_rooms,
    credit_card_transactions,
    expensed_items,
    github_repos,
    groups,
    inbound_shipments,
    journal_club_meetings,
    journal_club_papers,
    links,
    mailing_list_subscribers,
    outbound_shipments,
    page_views,
    rack_line_subscribers,
    recorded_meetings,
    rfds,
    software_vendors,
    swag_inventory_items,
    swag_items,
    users,
);
