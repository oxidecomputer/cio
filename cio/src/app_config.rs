use docusign::Envelope;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::applicants::Applicant;

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct DocuSignConfig {
    pub offer: Envelope,
    pub piia: Envelope,
}

impl DocuSignConfig {
    pub fn get_offer_letter(&self, applicant: &Applicant) -> Envelope {
        let mut new_envelope = self.offer.clone();

        for template_role in new_envelope.template_roles.iter_mut() {
            template_role.name = template_role.name.replace("{applicant_name}", &applicant.name);
            template_role.email = template_role.email.replace("{applicant_email}", &applicant.email);
            template_role.signer_name = template_role.signer_name.replace("{applicant_name}", &applicant.name);

            template_role.email_notification.email_subject = template_role
                .email_notification
                .email_subject
                .replace("{applicant_name}", &applicant.name);

            template_role.email_notification.email_subject = template_role
                .email_notification
                .email_body
                .replace("{applicant_name}", &applicant.name)
                .replace("{applicant_email}", &applicant.email);
        }

        new_envelope
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct OnboardingConfig {
    pub new_hire_issue: NewHireIssue,
    pub welcome_letter: Letter,
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
    use crate::applicants::Applicant;
    use super::DocuSignConfig;

    // fn mock_applicant() -> Applicant {
    //     Applicant {

    //     }
    // }

    #[test]
    fn populates_offer_letter() {
        // let applicant = mock_applicant();

        let offer = r#"
[offer]
status = 'sent'
emailSubject = 'Sign Offer Letter'
templateId = 'random-offer-id'

[[offer.templateRoles]]
name = 'Owner'
roleName = 'Owner'
email = 'owner_email'
signerName = 'The Owner'
routingOrder = '1'
emailNotification.emailSubject = 'Create offer letter for {applicant_name}'
emailNotification.emailBody = """Please create the offer letter for {applicant_name}.
Once it is created it will be sent to {applicant_name} at {applicant_email}."""
emailNotification.language = ''

[[offer.templateRoles]]
name = '{applicant_name}'
roleName = 'Applicant'
email = '{applicant_email}'
signerName = '{applicant_name}'
routingOrder = '2'
emailNotification.emailSubject = 'Sign the Offer Letter'
emailNotification.emailBody = """Please sign the letter"""
emailNotification.language = ''
"#;

        let config: DocuSignConfig = toml::from_str(offer).unwrap();

    }
}