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
}

impl Default for Status {
    fn default() -> Self {
        Status::Queued
    }
}

impl ToString for Status {
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
        }
    }
}
