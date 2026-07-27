use super::types::VehicleJobPayload;

/// Basic value normalization tables — expanded in M3.5 for Facebook form mapping.
pub fn normalize_payload_values(payload: &mut VehicleJobPayload) {
    payload.trim = payload.trim.trim().to_string();
    payload.stock_number = payload.stock_number.trim().to_string();
    payload.drivetrain = normalize_drivetrain(&payload.drivetrain);
    payload.condition = normalize_condition(&payload.condition);
    payload.transmission = normalize_transmission(&payload.transmission);
}

fn normalize_drivetrain(raw: &str) -> String {
    let upper = raw.trim().to_uppercase();
    match upper.as_str() {
        "FWD" | "FRONT-WHEEL DRIVE" | "FRONT WHEEL DRIVE" => "FWD".into(),
        "RWD" | "REAR-WHEEL DRIVE" | "REAR WHEEL DRIVE" => "RWD".into(),
        "AWD" | "ALL-WHEEL DRIVE" | "ALL WHEEL DRIVE" => "AWD".into(),
        "4WD" | "4X4" | "FOUR-WHEEL DRIVE" | "FOUR WHEEL DRIVE" => "4WD".into(),
        _ => raw.trim().to_string(),
    }
}

fn normalize_condition(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    match lower.as_str() {
        "new" => "New".into(),
        "used" => "Used".into(),
        "excellent" => "Excellent".into(),
        "good" => "Good".into(),
        "fair" => "Fair".into(),
        "salvage" => "Salvage".into(),
        _ if !raw.trim().is_empty() => {
            let mut chars = raw.trim().chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str().to_lowercase().as_str(),
            }
        }
        _ => String::new(),
    }
}

fn normalize_transmission(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    if lower.contains("auto") {
        return "Automatic transmission".into();
    }
    if lower.contains("manual") {
        return "Manual transmission".into();
    }
    raw.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketplace::payload::types::{ExecutionMode, ListingOptions};

    fn base_payload() -> VehicleJobPayload {
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
            trim: "  SE  ".into(),
            body_style: "Sedan".into(),
            vehicle_type: "car_truck".into(),
            condition: "excellent".into(),
            price: "20000".into(),
            mileage: "10000".into(),
            vin: "VIN".into(),
            stock_number: " 123 ".into(),
            exterior_color: "Blue".into(),
            interior_color: "Black".into(),
            transmission: "automatic".into(),
            drivetrain: "all-wheel drive".into(),
            fuel_type: "Gas".into(),
            title: "2022 Toyota Camry SE".into(),
            description: "Desc".into(),
            location: "Austin, TX".into(),
            ordered_image_urls: vec!["https://cdn.example/a.jpg".into()],
            listing_options: ListingOptions::default(),
            posting_preferences: serde_json::json!({}),
            execution_mode: ExecutionMode::PrepareForReview,
            idempotency_key: "key".into(),
            source_metadata: serde_json::json!({}),
            expires_at: "2026-07-27T12:00:00Z".into(),
            test: false,
            label: None,
            scoped_job_token: None,
        }
    }

    #[test]
    fn normalize_payload_values_trims_and_maps_enums() {
        let mut payload = base_payload();
        normalize_payload_values(&mut payload);
        assert_eq!(payload.trim, "SE");
        assert_eq!(payload.stock_number, "123");
        assert_eq!(payload.drivetrain, "AWD");
        assert_eq!(payload.condition, "Excellent");
        assert_eq!(payload.transmission, "Automatic transmission");
    }
}
