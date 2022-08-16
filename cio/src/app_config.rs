use docusign::Envelope;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{applicants::Applicant, companies::Company, configs::User};

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct DocuSignConfig {
    offer: Envelope,
    piia: Envelope,
}

impl DocuSignConfig {
    pub fn create_offer_letter(&self, applicant: &Applicant) -> Envelope {
        let mut envelope = self.offer.clone();
        DocuSignConfig::fill_envelope(&mut envelope, applicant);

        envelope
    }

    pub fn create_piia_letter(&self, applicant: &Applicant) -> Envelope {
        let mut envelope = self.piia.clone();
        DocuSignConfig::fill_envelope(&mut envelope, applicant);

        envelope
    }

    fn fill_envelope(envelope: &mut Envelope, applicant: &Applicant) {
        for template_role in envelope.template_roles.iter_mut() {
            template_role.name = template_role.name.replace("{applicant_name}", &applicant.name);
            template_role.email = template_role.email.replace("{applicant_email}", &applicant.email);
            template_role.signer_name = template_role.signer_name.replace("{applicant_name}", &applicant.name);

            template_role.email_notification.email_subject = template_role
                .email_notification
                .email_subject
                .replace("{applicant_name}", &applicant.name)
                .replace("{applicant_email}", &applicant.email);

            template_role.email_notification.email_body = template_role
                .email_notification
                .email_body
                .replace("{applicant_name}", &applicant.name)
                .replace("{applicant_email}", &applicant.email);
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct OnboardingConfig {
    pub new_hire_issue: NewHireIssue,
    pub welcome_letter: Letter,
}

impl OnboardingConfig {
    pub fn create_welcome_letter(&self, company: &Company, user: &User, password: &str) -> Letter {
        let mut letter = self.welcome_letter.clone();

        // Get the user's aliases if they have one.
        let aliases = user.aliases.join(", ");

        letter.subject = letter.subject.replace("{user_email}", &user.email);
        letter.body = letter
            .body
            .replace("{user_name}", &user.first_name)
            .replace("{company_domain}", &company.domain)
            .replace("{user_email}", &user.email)
            .replace("{user_password}", password)
            .replace("{user_aliases}", &aliases)
            .replace("{user_github}", &user.github)
            .replace("{company_github}", &company.github_org);

        letter.cc.push(user.email.to_string());

        letter
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct NewHireIssue {
    pub assignees: Vec<String>,
    pub alerts: Vec<String>,
    pub default_groups: Vec<String>,
    pub aws_roles: Vec<String>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Letter {
    pub subject: String,
    pub body: String,
    pub from: String,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ApplyConfig {
    pub received: Letter,
    pub rejection: HashMap<String, Letter>,
}

impl ApplyConfig {
    pub fn create_received_letter(&self, applicant: &Applicant) -> Letter {
        let mut letter = self.received.clone();
        letter.subject = letter
            .subject
            .replace("{applicant_name}", &applicant.name)
            .replace("{applicant_email}", &applicant.email)
            .replace("{applicant_role}", &applicant.role);
        letter.body = letter
            .body
            .replace("{applicant_name}", &applicant.name)
            .replace("{applicant_email}", &applicant.email)
            .replace("{applicant_role}", &applicant.role);

        letter
    }

    pub fn create_rejection_letter(&self, letter_key: &str, applicant: &Applicant) -> Option<Letter> {
        self.rejection.get(letter_key).map(|letter| {
            let mut letter = letter.clone();
            letter.subject = letter
                .subject
                .replace("{applicant_name}", &applicant.name)
                .replace("{applicant_email}", &applicant.email)
                .replace("{applicant_role}", &applicant.role);
            letter.body = letter
                .body
                .replace("{applicant_name}", &applicant.name)
                .replace("{applicant_email}", &applicant.email)
                .replace("{applicant_role}", &applicant.role);

            letter
        })
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct LegacyExpensifyConfig {
    pub aliases: HashMap<String, String>,
    pub emails_to_exclude: Vec<String>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct FinanceConfig {
    pub legacy_expensify: LegacyExpensifyConfig,
    pub merchant_aliases: HashMap<String, String>,
    pub vendor_aliases: HashMap<String, String>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub envelopes: DocuSignConfig,
    pub onboarding: OnboardingConfig,
    pub apply: ApplyConfig,
    pub finance: FinanceConfig,
}

#[cfg(test)]
mod tests {
    use super::{ApplyConfig, DocuSignConfig, OnboardingConfig};
    use crate::{applicants::tests::mock_applicant, companies::tests::mock_company, configs::tests::mock_user};

    fn mock_docusign_toml(label: &str) -> String {
        format!(
            r#"
[{label}]
status = 'sent'
emailSubject = 'Sign Letter'
templateId = 'random-id'

[[{label}.templateRoles]]
name = 'Owner'
roleName = 'Owner'
email = 'owner_email'
signerName = 'The Owner'
routingOrder = '1'
emailNotification.emailSubject = 'Create letter for {{applicant_name}}'
emailNotification.emailBody = """Please create the letter for {{applicant_name}}.
Once it is created it will be sent to {{applicant_name}} at {{applicant_email}}."""
emailNotification.language = ''

[[{label}.templateRoles]]
name = '{{applicant_name}}'
roleName = 'Applicant'
email = '{{applicant_email}}'
signerName = '{{applicant_name}}'
routingOrder = '2'
emailNotification.emailSubject = 'Sign the Letter {{applicant_name}}'
emailNotification.emailBody = """Please sign the letter {{applicant_name}}"""
emailNotification.language = ''
"#
        )
    }

    fn mock_docusign_config() -> DocuSignConfig {
        let offer = mock_docusign_toml("offer");
        let piia = mock_docusign_toml("piia");

        toml::from_str(&format!("{}\n{}", offer, piia)).unwrap()
    }

    #[test]
    fn test_populates_letter() {
        let applicant = mock_applicant();

        let config = mock_docusign_config();

        let expected_owner_subject = "Create letter for Test User".to_string();
        let expected_owner_body = r#"Please create the letter for Test User.
Once it is created it will be sent to Test User at random-test@testemaildomain.com."#
            .to_string();

        let expected_recipient_subject = "Sign the Letter Test User".to_string();
        let expected_recipient_body = "Please sign the letter Test User".to_string();

        let envelope = config.create_offer_letter(&applicant);

        assert_eq!(
            expected_owner_subject,
            envelope.template_roles[0].email_notification.email_subject
        );
        assert_eq!(
            expected_owner_body,
            envelope.template_roles[0].email_notification.email_body
        );

        assert_eq!(applicant.name, envelope.template_roles[1].name);
        assert_eq!(applicant.email, envelope.template_roles[1].email);
        assert_eq!(applicant.name, envelope.template_roles[1].signer_name);
        assert_eq!(
            expected_recipient_subject,
            envelope.template_roles[1].email_notification.email_subject
        );
        assert_eq!(
            expected_recipient_body,
            envelope.template_roles[1].email_notification.email_body
        );
    }

    fn mock_apply_toml() -> &'static str {
        r#"
[received]
subject = 'Received for {applicant_name} ({applicant_role})'
body = '{applicant_name} [{applicant_email}], thank you for applying for {applicant_role}.'
from = 'test@testemaildomain.com'

[rejection.test-rejection]
subject = 'Rejection for {applicant_name} ({applicant_role})'
body = '{applicant_name} [{applicant_email}], thank you for your interest in {applicant_role}.'
from = 'test@testemaildomain.com'
"#
    }

    #[test]
    fn test_received_letter() {
        let config: ApplyConfig = toml::from_str(&mock_apply_toml()).unwrap();
        let applicant = mock_applicant();

        let letter = config.create_received_letter(&applicant);

        assert_eq!("Received for Test User (Engineering)", letter.subject);
        assert_eq!(
            "Test User [random-test@testemaildomain.com], thank you for applying for Engineering.",
            letter.body
        );
    }

    #[test]
    fn test_rejection_letter() {
        let config: ApplyConfig = toml::from_str(&mock_apply_toml()).unwrap();
        let applicant = mock_applicant();

        let letter = config.create_rejection_letter("test-rejection", &applicant).unwrap();

        assert_eq!("Rejection for Test User (Engineering)", letter.subject);
        assert_eq!(
            "Test User [random-test@testemaildomain.com], thank you for your interest in Engineering.",
            letter.body
        );
    }

    #[test]
    fn test_missing_rejection_letter() {
        let config: ApplyConfig = toml::from_str(&mock_apply_toml()).unwrap();
        let applicant = mock_applicant();
        let letter = config.create_rejection_letter("test-missing", &applicant);

        assert!(letter.is_none());
    }

    fn mock_onboarding_toml() -> &'static str {
        r#"
[new_hire_issue]
assignees = []
alerts = []
default_groups = []
aws_roles = []

[welcome_letter]
subject = 'Welcome {user_email}'
body = """Welcome {user_name}. Here is your information
username: {user_name}
domain: {company_domain}
email: {user_email}
password: {user_password}
aliases: {user_aliases}
github: {user_github}
github org: {company_github}
"""
from = 'test@testemaildomain.com'
cc = ['first@testemaildomain.com']
"#
    }

    #[test]
    fn test_welcome_letter() {
        let config: OnboardingConfig = toml::from_str(mock_onboarding_toml()).unwrap();
        let company = mock_company();
        let user = mock_user();
        let letter = config.create_welcome_letter(&company, &user, "new_user_password");

        assert_eq!("Welcome random-test@testemaildomain.com", letter.subject);
        assert_eq!(
            r#"Welcome random. Here is your information
username: random
domain: super.computer
email: random-test@testemaildomain.com
password: new_user_password
aliases: al1, al2
github: random_github_user
github org: super_computer_org
"#,
            letter.body
        );
        assert_eq!(
            vec!["first@testemaildomain.com", "random-test@testemaildomain.com"],
            letter.cc
        );
    }
}
