use docusign::Envelope;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct DocuSignConfig {
    pub offer: Envelope,
    pub piia: Envelope,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct OnboardingConfig {
    pub new_hire_issue: NewHireIssue,
    pub welcome_letter: Letter
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
pub struct VendorConfig {
    pub alias: HashMap<String, String>
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub envelopes: DocuSignConfig,
    pub onboarding: OnboardingConfig,
    pub apply: ApplyConfig,
    pub vendors: VendorConfig
}