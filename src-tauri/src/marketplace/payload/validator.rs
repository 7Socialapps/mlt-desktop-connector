use super::normalize::normalize_payload_values;
use super::types::{ExecutionMode, VehicleJobPayload};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub field: String,
    pub code: String,
    pub message: String,
}

pub fn validate_payload(payload: &VehicleJobPayload) -> Vec<ValidationError> {
    if payload.is_transport_test() {
        return validate_transport_test(payload);
    }

    let mut errors = Vec::new();
    require_str(&mut errors, "job_id", &payload.job_id);
    require_str(&mut errors, "user_id", &payload.user_id);
    require_str(&mut errors, "dealership_id", &payload.dealership_id);
    require_str(&mut errors, "inventory_id", &payload.inventory_id);
    require_str(&mut errors, "inventory_source", &payload.inventory_source);
    require_str(&mut errors, "year", &payload.year);
    require_str(&mut errors, "make", &payload.make);
    require_str(&mut errors, "model", &payload.model);
    require_str(&mut errors, "body_style", &payload.body_style);
    require_str(&mut errors, "vehicle_type", &payload.vehicle_type);
    require_str(&mut errors, "condition", &payload.condition);
    require_str(&mut errors, "price", &payload.price);
    require_str(&mut errors, "mileage", &payload.mileage);
    require_str(&mut errors, "vin", &payload.vin);
    require_str(&mut errors, "transmission", &payload.transmission);
    require_str(&mut errors, "fuel_type", &payload.fuel_type);
    require_str(&mut errors, "title", &payload.title);
    require_str(&mut errors, "description", &payload.description);
    require_str(&mut errors, "location", &payload.location);
    require_str(&mut errors, "idempotency_key", &payload.idempotency_key);
    require_str(&mut errors, "expires_at", &payload.expires_at);

    if payload.contract_version < 1 {
        errors.push(ValidationError {
            field: "contract_version".into(),
            code: "INVALID".into(),
            message: "contract_version must be >= 1".into(),
        });
    }

    if payload.ordered_image_urls.is_empty() {
        errors.push(ValidationError {
            field: "ordered_image_urls".into(),
            code: "NO_IMAGES".into(),
            message: "ordered_image_urls must contain at least one URL".into(),
        });
    }

    if !payload.execution_mode.is_supported_in_m3() {
        errors.push(ValidationError {
            field: "execution_mode".into(),
            code: "UNSUPPORTED_EXECUTION_MODE".into(),
            message: format!(
                "execution_mode '{}' is not supported on desktop in M3",
                payload.execution_mode.as_str()
            ),
        });
    }

    errors
}

pub fn validate_and_normalize(payload: &mut VehicleJobPayload) -> Vec<ValidationError> {
    normalize_payload_values(payload);
    validate_payload(payload)
}

fn validate_transport_test(payload: &VehicleJobPayload) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    require_str(&mut errors, "job_id", &payload.job_id);
    if !matches!(
        payload.execution_mode,
        ExecutionMode::TransportTest
    ) && !payload.test
    {
        errors.push(ValidationError {
            field: "execution_mode".into(),
            code: "INVALID".into(),
            message: "transport test jobs must set execution_mode=transport_test or test=true"
                .into(),
        });
    }
    errors
}

fn require_str(errors: &mut Vec<ValidationError>, field: &str, value: &str) {
    if value.trim().is_empty() {
        errors.push(ValidationError {
            field: field.to_string(),
            code: "MISSING".into(),
            message: format!("{field} is required"),
        });
    }
}

pub fn reject_unsupported_execution_mode(mode: ExecutionMode) -> Option<ValidationError> {
    if mode.is_supported_in_m3() {
        return None;
    }
    Some(ValidationError {
        field: "execution_mode".into(),
        code: "UNSUPPORTED_EXECUTION_MODE".into(),
        message: format!(
            "execution_mode '{}' is rejected before browser automation in M3",
            mode.as_str()
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketplace::payload::types::ListingOptions;

    fn full_payload(mode: ExecutionMode) -> VehicleJobPayload {
        VehicleJobPayload {
            contract_version: 1,
            job_id: "job-1".into(),
            user_id: "user-1".into(),
            dealership_id: "dealer-1".into(),
            inventory_id: "inv-1".into(),
            inventory_source: "ctl".into(),
            year: "2022".into(),
            make: "Toyota".into(),
            model: "Camry".into(),
            trim: "SE".into(),
            body_style: "Sedan".into(),
            vehicle_type: "car_truck".into(),
            condition: "Excellent".into(),
            price: "20000".into(),
            mileage: "10000".into(),
            vin: "VIN123".into(),
            stock_number: "STK".into(),
            exterior_color: "Blue".into(),
            interior_color: "Black".into(),
            transmission: "Automatic".into(),
            drivetrain: "AWD".into(),
            fuel_type: "Gas".into(),
            title: "2022 Toyota Camry SE".into(),
            description: "Clean car".into(),
            location: "Austin, TX".into(),
            ordered_image_urls: vec!["https://cdn.example/a.jpg".into()],
            listing_options: ListingOptions::default(),
            posting_preferences: serde_json::json!({}),
            execution_mode: mode,
            idempotency_key: "key".into(),
            source_metadata: serde_json::json!({}),
            expires_at: "2026-07-27T12:00:00Z".into(),
            test: false,
            label: None,
            scoped_job_token: None,
        }
    }

    #[test]
    fn valid_prepare_for_review_payload_passes_validation() {
        let payload = full_payload(ExecutionMode::PrepareForReview);
        assert!(validate_payload(&payload).is_empty());
    }

    #[test]
    fn missing_required_fields_fail_before_browser() {
        let mut payload = full_payload(ExecutionMode::PrepareForReview);
        payload.vin = String::new();
        payload.location = String::new();
        let errors = validate_payload(&payload);
        assert!(errors.iter().any(|e| e.field == "vin"));
        assert!(errors.iter().any(|e| e.field == "location"));
    }

    #[test]
    fn transport_test_still_works_with_minimal_fields() {
        let payload = VehicleJobPayload {
            contract_version: 1,
            job_id: "job-test".into(),
            user_id: String::new(),
            dealership_id: String::new(),
            inventory_id: String::new(),
            inventory_source: String::new(),
            year: String::new(),
            make: String::new(),
            model: String::new(),
            trim: String::new(),
            body_style: String::new(),
            vehicle_type: String::new(),
            condition: String::new(),
            price: String::new(),
            mileage: String::new(),
            vin: String::new(),
            stock_number: String::new(),
            exterior_color: String::new(),
            interior_color: String::new(),
            transmission: String::new(),
            drivetrain: String::new(),
            fuel_type: String::new(),
            title: String::new(),
            description: String::new(),
            location: String::new(),
            ordered_image_urls: vec![],
            listing_options: ListingOptions::default(),
            posting_preferences: serde_json::json!({}),
            execution_mode: ExecutionMode::TransportTest,
            idempotency_key: String::new(),
            source_metadata: serde_json::json!({}),
            expires_at: String::new(),
            test: true,
            label: Some("Transport test".into()),
            scoped_job_token: None,
        };
        assert!(validate_payload(&payload).is_empty());
    }

    #[test]
    fn publish_after_review_and_fully_automatic_rejected() {
        for mode in [
            ExecutionMode::PublishAfterReview,
            ExecutionMode::FullyAutomatic,
        ] {
            let payload = full_payload(mode);
            let err = reject_unsupported_execution_mode(mode).expect("should reject");
            assert_eq!(err.code, "UNSUPPORTED_EXECUTION_MODE");
            let validation = validate_payload(&payload);
            assert!(validation.iter().any(|e| e.field == "execution_mode"));
        }
    }
}
