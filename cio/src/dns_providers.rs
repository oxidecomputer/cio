use anyhow::Result;
use async_trait::async_trait;
use std::fmt;

pub struct DnsRecord {
    pub name: String,
    pub type_: DnsRecordType,
    pub content: String,
}

// We only support adding and removing a subset of the possible DNS types
pub enum DnsRecordType {
    A,
    AAAA,
    CNAME,
    MX,
    NS,
    SRV,
    TXT,
}

impl fmt::Display for DnsRecordType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::A => write!(f, "A"),
            Self::AAAA => write!(f, "AAAA"),
            Self::CNAME => write!(f, "CNAME"),
            Self::NS => write!(f, "NS"),
            Self::MX => write!(f, "MX"),
            Self::TXT => write!(f, "TXT"),
            Self::SRV => write!(f, "SRV"),
        }
    }
}

/// This trait defines how to implement a provider for a vendor that manages DNS records.
#[async_trait]
pub trait DNSProviderOps {
    /// Ensure the record exists and has the correct information.
    async fn ensure_record(&self, record: DnsRecord) -> Result<()>;

    /// Delete the record if it exists.
    async fn delete_record(&self, record: DnsRecord) -> Result<()>;
}
