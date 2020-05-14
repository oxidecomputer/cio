use std::env;
use std::rc::Rc;

use reqwest::blocking::{Client, Request};
use reqwest::{header, Method, StatusCode, Url};
use serde::Serialize;

use crate::core::Applicant;
use crate::directory::core::User;
use crate::email::core::{Content, Email, Message, Personalization};

const ENDPOINT: &str = "https://api.sendgrid.com/v3/";

pub struct SendGrid {
    key: String,
    domain: String,
    github_org: String,

    client: Rc<Client>,
}

impl SendGrid {
    // Create a new SendGrid client struct. It takes a type that can convert into
    // an &str (`String` or `Vec<u8>` for example). As long as the function is
    // given a valid API Key and Secret your requests will work.
    pub fn new<K, D, G>(key: K, domain: D, github_org: G) -> Self
    where
        K: ToString,
        D: ToString,
        G: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                key: key.to_string(),
                domain: domain.to_string(),
                github_org: github_org.to_string(),

                client: Rc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    pub fn new_from_env() -> Self {
        let key = env::var("SENDGRID_API_KEY").unwrap();
        let domain = env::var("EMAIL_TEMPLATE_DOMAIN").unwrap();
        let github_org = env::var("GITHUB_ORG").unwrap();

        return SendGrid::new(key, domain, github_org);
    }

    // Get the currently set API key.
    pub fn get_key(&self) -> &str {
        &self.key
    }

    pub fn request<B>(
        &self,
        method: Method,
        path: String,
        body: B,
        query: Option<Vec<(&str, String)>>,
    ) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(&path).unwrap();

        let bt = format!("Bearer {}", self.key);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(
            header::CONTENT_TYPE,
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

    pub fn send_mail(&self, message: Message) {
        // Build the request.
        let request = self.request(Method::POST, "mail/send".to_string(), message, None);

        let resp = self.client.execute(request).unwrap();
        match resp.status() {
            StatusCode::ACCEPTED => (),
            s => panic!("received response status: {:?}", s),
        };
    }

    pub fn send_new_user(&self, user: User, password: String, github: String) {
        // Get the user's aliases if they have one.
        let mut aliases: Vec<String> = Default::default();
        match user.clone().aliases {
            None => (),
            Some(val) => aliases = val.clone(),
        }

        // Create the message.
        let admin_email = format!("admin@{}", self.domain);

        let message = Message::new()
            .set_from(Email::new().set_email(&admin_email).set_name(&admin_email))
            .set_subject(&format!(
                "Your New Email Account: {}",
                user.clone().primary_email.unwrap()
            ))
            .add_content(
                Content::new()
                    .set_content_type("text/plain")
                    .set_value(&format!(
                        "Yoyoyo {},

We have set up your account on mail.corp.{}. Details for accessing
are below. You will be required to reset your password the next time you login.

Website for Login: https://mail.corp.{}
Email: {}
Password: {}
Aliases: {}

Make sure you set up two-factor authentication for your account, or in one week
you will be locked out.

Your GitHub @{} has been added to our organization (https://github.com/{})
and various teams within it. GitHub should have sent an email with instructions on
accepting the invitation to our organization to the email you used
when you signed up for GitHub. Or you can alternatively accept our invitation
by going to https://github.com/{}.

You will be invited to create a Zoom account from an email sent to {}. Once
completed, your personal URL for Zoom calls will be https://oxide.zoom.us/my/{}.

If you have any questions or your email does not work please email your
administrator, who is cc-ed on this email. Spoiler alert it's Jess...
jess@{}. If you want other email aliases, let Jess know as well.

Once you login to your email, a great place to start would be taking a look at
our on-boarding doc:
https://docs.google.com/document/d/18Nymnd3rU1Nz4woxPfcohFeyouw7FvbYq5fGfQ6ZSGY/edit?usp=sharing.

xoxo,
  The GSuite/GitHub/Zoom Bot",
                        user.clone().name.unwrap().given_name.unwrap(),
                        self.domain,
                        self.domain,
                        user.clone().primary_email.unwrap(),
                        password,
                        aliases.join(", "),
                        github.clone(),
                        self.github_org,
                        self.github_org,
                        user.clone().primary_email.unwrap(),
                        github,
                        self.domain
                    )),
            )
            .add_personalization(
                Personalization::new()
                    .add_to(
                        Email::new()
                            .set_email(&user.recovery_email.unwrap())
                            .set_name(&user.name.unwrap().full_name.unwrap()),
                    )
                    .add_cc(Email::new().set_email(&format!("jess@{}", self.domain))),
            );

        // Send the message.
        self.send_mail(message);
    }

    pub fn send_received_application(&self, email: &str, name: &str) {
        let careers_email = format!("careers@{}", self.domain);

        // Create the message.
        let message = Message::new()
            .set_from(
                Email::new()
                    .set_email(&careers_email)
                    .set_name(&careers_email),
            )
            .set_subject("Oxide Computer Company Application Received!")
            .add_content(Content::new().set_content_type("text/plain").set_value(
                "Thank you for submitting your application materials! We really appreciate all
the time and thought everyone puts into their application. We will be in touch
within the next couple weeks with more information.

Sincerely,
  The Oxide Team",
            ))
            .add_personalization(
                Personalization::new()
                    .add_to(Email::new().set_email(email).set_name(name))
                    .add_cc(Email::new().set_email(&careers_email)),
            );

        // Send the message.
        self.send_mail(message);
    }

    pub fn send_uploaded_zoom_dump(&self, drive_url: &str) {
        let drive_email = format!("drive@{}", self.domain);

        // Create the message.
        let message = Message::new()
            .set_from(Email::new().set_email(&drive_email).set_name(&drive_email))
            .set_subject("New Zoom meeting video upload!")
            .add_content(
                Content::new()
                    .set_content_type("text/plain")
                    .set_value(&format!(
                        "Zoom videos have been uploaded to: {}. You might want to sort them!",
                        drive_url
                    )),
            )
            .add_personalization(
                Personalization::new()
                    .add_to(
                        Email::new()
                            .set_email(&format!("jess@{}", self.domain))
                            .set_name(&format!("jess@{}", self.domain)),
                    )
                    .add_cc(Email::new().set_email(&drive_email)),
            );

        // Send the message.
        self.send_mail(message);
    }

    pub fn send_new_applicant_notification(&self, applicant: Applicant) {
        let applications_email = format!("applications@{}", self.domain);
        let all_email = format!("all@{}", self.domain);

        // Create the message.
        let message = Message::new()
            .set_from(
                Email::new()
                    .set_email(&applications_email)
                    .set_name(&applications_email),
            )
            .set_subject(&format!("New Application: {}", applicant.clone().name,))
            .add_content(
                Content::new()
                    .set_content_type("text/plain")
                    .set_value(&format!(
                        "## Applicant Information

Submitted Date: {}
Name: {}
Email: {}
Phone: {}
Location: {}
GitHub: {}
Resume: {}
Oxide Candidate Materials: {}

## Reminder

To view the all the candidates refer to the following Google spreadsheets:

- Engineering Applications: https://docs.google.com/spreadsheets/d/1FHA-otHCGwe5fCRpcl89MWI7GHiFfN3EWjO6K943rYA/edit?usp=sharing
- Product Engineering and Design Applications: https://docs.google.com/spreadsheets/d/1VkRgmr_ZdR-y_1NJc8L0Iv6UVqKaZapt3T_Bq_gqPiI/edit?usp=sharing
",
                        applicant.clone().submitted_time,
                        applicant.clone().name,
                        applicant.clone().email,
                        applicant.clone().phone,
                        applicant.clone().location,
                        applicant.clone().github,
                        applicant.clone().resume,
                        applicant.clone().materials,
                    )),
            )
            .add_personalization(
                Personalization::new()
                    .add_to(
                        Email::new()
                            .set_email(&all_email)
                            .set_name(&all_email),
                    )
            );

        // Send the message.
        self.send_mail(message);
    }
}
