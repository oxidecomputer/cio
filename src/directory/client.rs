use std::collections::HashMap;
use std::rc::Rc;

use reqwest::blocking::{Client, Request};
use reqwest::{header, Method, StatusCode, Url};
use serde::Serialize;
use yup_oauth2::Token;

use crate::directory::core::{
    Building, Buildings, CalendarResource, CalendarResources, Group, GroupSettings, Groups, Member,
    MembersHasMember, User, Users,
};

const ENDPOINT: &str = "https://www.googleapis.com/admin/directory/v1/";
const SETTINGS_ENDPOINT: &str = "https://www.googleapis.com/groups/v1/groups/";

pub struct Directory {
    customer: String,
    domain: String,

    token: Token,

    client: Rc<Client>,
}

impl Directory {
    // Create a new Directory client struct. It takes a type that can convert into
    // an &str (`String` or `Vec<u8>` for example). As long as the function is
    // given a valid API Key and Secret your requests will work.
    pub fn new(customer: String, domain: String, token: Token) -> Self {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                customer: customer,
                domain: domain,
                token: token,
                client: Rc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    // Get the currently set authorization token.
    pub fn get_token(&self) -> &Token {
        &self.token
    }

    pub fn request<B>(
        &self,
        endpoint: &str,
        method: Method,
        path: String,
        body: B,
        query: Option<Vec<(&str, String)>>,
    ) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(endpoint).unwrap();
        let url = base.join(&path).unwrap();

        // Check if the token is expired and panic.
        if self.token.expired() {
            panic!("token is expired");
        }

        let bt = format!("Bearer {}", self.token.access_token);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        headers.append(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );

        let mut rb = self.client.request(method.clone(), url).headers(headers);

        match query {
            None => (),
            Some(val) => {
                rb = rb.query(&val);
            }
        }

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if method != Method::GET && method != Method::DELETE {
            rb = rb.json(&body);
        }

        // Build the request.
        let request = rb.build().unwrap();

        return request;
    }

    pub fn list_groups(&self) -> Vec<Group> {
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::GET,
            "groups".to_string(),
            {},
            Some(vec![
                ("customer", self.customer.to_string()),
                ("domain", self.domain.to_string()),
            ]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        let value: Groups = resp.json().unwrap();

        return value.groups.unwrap();
    }

    pub fn get_group_settings(&self, group_email: String) -> GroupSettings {
        // Build the request.
        let request = self.request(
            SETTINGS_ENDPOINT,
            Method::GET,
            group_email,
            {},
            Some(vec![("alt", "json".to_string())]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        return resp.json().unwrap();
    }

    pub fn update_group(&self, group: Group) {
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::PUT,
            format!("groups/{}", group.clone().id.unwrap()),
            group,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    pub fn update_group_settings(&self, settings: GroupSettings) {
        // Build the request.
        let request = self.request(
            SETTINGS_ENDPOINT,
            Method::PUT,
            settings.clone().email.unwrap(),
            settings,
            Some(vec![("alt", "json".to_string())]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    pub fn create_group(&self, group: Group) -> Group {
        // Build the request.
        let request = self.request(ENDPOINT, Method::POST, "groups".to_string(), group, None);

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        return resp.json().unwrap();
    }

    pub fn update_group_aliases(&self, group_key: String, aliases: Vec<String>) {
        for alias in aliases {
            self.update_group_alias(group_key.to_string(), alias.to_string());
        }
    }

    pub fn update_group_alias(&self, group_key: String, alias: String) {
        let mut a: HashMap<String, String> = HashMap::new();
        a.insert("alias".to_string(), alias);
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::POST,
            format!("groups/{}/aliases", group_key.to_string()),
            a,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                let body = resp.text().unwrap();

                if body.contains("duplicate") {
                    // Ignore the error because we don't care about if it is a duplicate.
                    return;
                }

                panic!(
                    "received response status: {:?}\nresponse body: {:?}",
                    s, body,
                );
            }
        };
    }

    pub fn group_has_member(&self, group_id: String, email: String) -> bool {
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::GET,
            format!("groups/{}/hasMember/{}", group_id, email),
            {},
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        let value: MembersHasMember = resp.json().unwrap();

        return value.is_member.unwrap();
    }

    pub fn group_update_member(&self, group_id: String, email: String, role: String) {
        let mut member: Member = Default::default();
        member.role = Some(role.to_string());
        member.email = Some(email.to_string());
        member.delivery_settings = Some("ALL_MAIL".to_string());

        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::PUT,
            format!("groups/{}/members/{}", group_id, email.to_string()),
            member,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    pub fn group_insert_member(&self, group_id: String, email: String, role: String) {
        let mut member: Member = Default::default();
        member.role = Some(role.to_string());
        member.email = Some(email.to_string());
        member.delivery_settings = Some("ALL_MAIL".to_string());

        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::POST,
            format!("groups/{}/members", group_id),
            member,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    pub fn group_remove_member(&self, group_id: String, email: String) {
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::DELETE,
            format!("groups/{}/members/{}", group_id, email),
            {},
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    pub fn list_users(&self) -> Vec<User> {
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::GET,
            "users".to_string(),
            {},
            Some(vec![
                ("customer", self.customer.to_string()),
                ("domain", self.domain.to_string()),
                ("projection", "full".to_string()),
            ]),
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        let value: Users = resp.json().unwrap();

        return value.users.unwrap();
    }

    pub fn update_user(&self, user: User) {
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::PUT,
            format!("users/{}", user.clone().id.unwrap()),
            user,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    pub fn create_user(&self, user: User) -> User {
        // Build the request.
        let request = self.request(ENDPOINT, Method::POST, "users".to_string(), user, None);

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        return resp.json().unwrap();
    }

    pub fn update_user_aliases(&self, user_id: String, aliases: Vec<String>) {
        for alias in aliases {
            self.update_user_alias(user_id.to_string(), alias.to_string());
        }
    }

    pub fn update_user_alias(&self, user_id: String, alias: String) {
        let mut a: HashMap<String, String> = HashMap::new();
        a.insert("alias".to_string(), alias);
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::POST,
            format!("users/{}/aliases", user_id.to_string()),
            a,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                let body = resp.text().unwrap();

                if body.contains("duplicate") {
                    // Ignore the error because we don't care about if it is a duplicate.
                    return;
                }

                panic!(
                    "received response status: {:?}\nresponse body: {:?}",
                    s, body,
                );
            }
        };
    }

    pub fn list_resources(&self) -> Vec<CalendarResource> {
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::GET,
            format!("customer/{}/resources/calendars", self.customer),
            {},
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        let value: CalendarResources = resp.json().unwrap();

        return value.items.unwrap();
    }

    pub fn update_resource(&self, resource: CalendarResource) {
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::PUT,
            format!(
                "customer/{}/resources/calendars/{}",
                self.customer,
                resource.clone().id
            ),
            resource,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    pub fn create_resource(&self, resource: CalendarResource) {
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::POST,
            format!("customer/{}/resources/calendars", self.customer),
            resource,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    pub fn list_buildings(&self) -> Vec<Building> {
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::GET,
            format!("customer/{}/resources/buildings", self.customer),
            {},
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };

        // Try to deserialize the response.
        let value: Buildings = resp.json().unwrap();

        return value.buildings.unwrap();
    }

    pub fn update_building(&self, building: Building) {
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::PUT,
            format!(
                "customer/{}/resources/buildings/{}",
                self.customer,
                building.clone().id
            ),
            building,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }

    pub fn create_building(&self, building: Building) {
        // Build the request.
        let request = self.request(
            ENDPOINT,
            Method::POST,
            format!("customer/{}/resources/buildings", self.customer),
            building,
            None,
        );

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => panic!(
                "received response status: {:?}\nbody: {}",
                s,
                resp.text().unwrap()
            ),
        };
    }
}
