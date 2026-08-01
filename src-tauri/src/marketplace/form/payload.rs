use std::collections::HashMap;

use crate::marketplace::payload::{ListingOptions, VehicleJobPayload};

/// Maps a normalized job payload to sidecar fill_vehicle_form fields.
pub fn vehicle_fill_payload_from_job(payload: &VehicleJobPayload) -> HashMap<String, String> {
    let mut map = HashMap::new();

    let category = payload
        .listing_options
        .vehicle_category
        .trim();
    if !category.is_empty() {
        map.insert("category".into(), category.to_string());
    } else if !payload.vehicle_type.is_empty() {
        map.insert("category".into(), payload.vehicle_type.clone());
    } else {
        map.insert("category".into(), "Car/Truck".into());
    }

    insert_if_nonempty(&mut map, "year", &payload.year);
    insert_if_nonempty(&mut map, "make", &payload.make);
    insert_if_nonempty(&mut map, "model", &payload.model);
    insert_if_nonempty(&mut map, "trim", &payload.trim);
    insert_if_nonempty(&mut map, "price", &payload.price);
    if payload.listing_options.include_mileage || !payload.mileage.is_empty() {
        insert_if_nonempty(&mut map, "mileage", &payload.mileage);
    }
    insert_if_nonempty(&mut map, "body_style", &payload.body_style);
    insert_if_nonempty(&mut map, "condition", &payload.condition);
    insert_if_nonempty(&mut map, "exterior_color", &payload.exterior_color);
    insert_if_nonempty(&mut map, "interior_color", &payload.interior_color);
    insert_if_nonempty(&mut map, "transmission", &payload.transmission);
    insert_if_nonempty(&mut map, "drivetrain", &payload.drivetrain);
    insert_if_nonempty(&mut map, "fuel_type", &payload.fuel_type);
    insert_if_nonempty(&mut map, "title", &payload.title);
    insert_if_nonempty(&mut map, "description", &payload.description);
    insert_if_nonempty(&mut map, "location", &payload.location);

    map
}

fn insert_if_nonempty(map: &mut HashMap<String, String>, key: &str, value: &str) {
    let trimmed = value.trim();
    if !trimmed.is_empty() {
        map.insert(key.to_string(), trimmed.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketplace::payload::{ExecutionMode, ListingOptions};

    fn sample_payload() -> VehicleJobPayload {
        VehicleJobPayload {
            contract_version: 1,
            job_id: "job-1".into(),
            user_id: String::new(),
            dealership_id: String::new(),
            inventory_id: String::new(),
            inventory_source: String::new(),
            year: "2020".into(),
            make: "Toyota".into(),
            model: "Camry".into(),
            trim: "SE".into(),
            body_style: "Sedan".into(),
            vehicle_type: "Car/Truck".into(),
            condition: "Excellent".into(),
            price: "25000".into(),
            mileage: "45000".into(),
            vin: String::new(),
            stock_number: String::new(),
            exterior_color: "Black".into(),
            interior_color: "Gray".into(),
            transmission: "Automatic".into(),
            drivetrain: "FWD".into(),
            fuel_type: "Gasoline".into(),
            title: "2020 Toyota Camry SE".into(),
            description: "Clean title".into(),
            location: "Austin, TX".into(),
            ordered_image_urls: vec![],
            listing_options: ListingOptions {
                include_mileage: true,
                ..ListingOptions::default()
            },
            posting_preferences: serde_json::json!({}),
            execution_mode: ExecutionMode::PrepareForReview,
            idempotency_key: String::new(),
            source_metadata: serde_json::json!({}),
            expires_at: String::new(),
            test: false,
            label: None,
            scoped_job_token: None,
        }
    }

    #[test]
    fn maps_core_vehicle_fields() {
        let map = vehicle_fill_payload_from_job(&sample_payload());
        assert_eq!(map.get("year").map(String::as_str), Some("2020"));
        assert_eq!(map.get("make").map(String::as_str), Some("Toyota"));
        assert_eq!(map.get("price").map(String::as_str), Some("25000"));
        assert_eq!(map.get("mileage").map(String::as_str), Some("45000"));
    }

    #[test]
    fn defaults_category_when_options_empty() {
        let mut p = sample_payload();
        p.listing_options.vehicle_category = String::new();
        p.vehicle_type = "SUV".into();
        let map = vehicle_fill_payload_from_job(&p);
        assert_eq!(map.get("category").map(String::as_str), Some("SUV"));
    }
}
