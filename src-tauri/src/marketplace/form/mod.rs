pub mod payload;
pub mod types;

pub use payload::vehicle_fill_payload_from_job;
pub use types::{
    FieldFillResult, FormFillReport, FormVerificationReport, ImageUploadReport,
    ImageUploadResult,
};
