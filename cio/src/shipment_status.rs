/// The various different statuses that an shipment can be in.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Status {
    LabelCreated,
    LabelPrinted,
    PickedUp,
    Shipped,
    Delivered,
    Error,
    Queued,
    WaitingForPickup,
    Returned,
    Failure,
    Cancelled,
    ImportReview,
    CleanSweeped,
    OnHold,
    None,
    Processing,
    PartiallyFulfilled,
}

impl Default for Status {
    #[tracing::instrument]
    fn default() -> Self {
        Status::Queued
    }
}

impl ToString for Status {
    #[tracing::instrument]
    fn to_string(&self) -> String {
        match self {
            Status::LabelCreated => "Label created".to_string(),
            Status::LabelPrinted => "Label printed".to_string(),
            Status::PickedUp => "Picked up".to_string(),
            Status::Shipped => "Shipped".to_string(),
            Status::Delivered => "Delivered".to_string(),
            Status::Error => "ERROR".to_string(),
            Status::Queued => "Queued".to_string(),
            Status::WaitingForPickup => "Waiting for pickup".to_string(),
            Status::Returned => "Returned".to_string(),
            Status::Failure => "Failure".to_string(),
            Status::Cancelled => "Cancelled".to_string(),
            Status::ImportReview => "Import review".to_string(),
            Status::CleanSweeped => "Clean sweeped".to_string(),
            Status::OnHold => "On hold".to_string(),
            Status::None => "None".to_string(),
            Status::Processing => "Processing".to_string(),
            Status::PartiallyFulfilled => "Partially fulfilled".to_string(),
        }
    }
}
