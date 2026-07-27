/**
 * Versioned Facebook Marketplace vehicle-create selector registry (v1).
 * Label-first patterns — resilient to compiled class name churn.
 */
export const SELECTOR_VERSION = "1";

/** @typedef {"text"|"combobox"|"textarea"|"location"} ControlKind */

/**
 * @typedef {{
 *   key: string,
 *   labels: string[],
 *   control: ControlKind,
 *   dependsOn?: string[],
 *   ariaPatterns?: string[],
 *   cssFallbacks?: string[],
 *   optional?: boolean,
 *   normalize?: (value: string) => string,
 * }} FieldDefinition
 */

/** @type {FieldDefinition[]} */
export const VEHICLE_CREATE_FIELDS = [
  {
    key: "listing_type",
    labels: ["Vehicle type", "Listing type", "Type of listing"],
    control: "combobox",
    optional: true,
  },
  {
    key: "category",
    labels: ["Category"],
    control: "combobox",
    ariaPatterns: ["category"],
  },
  {
    key: "year",
    labels: ["Year"],
    control: "combobox",
    dependsOn: ["category"],
    ariaPatterns: ["year"],
  },
  {
    key: "make",
    labels: ["Make"],
    control: "combobox",
    dependsOn: ["year"],
    ariaPatterns: ["make"],
  },
  {
    key: "model",
    labels: ["Model"],
    control: "combobox",
    dependsOn: ["make"],
    ariaPatterns: ["model"],
  },
  {
    key: "trim",
    labels: ["Trim"],
    control: "combobox",
    dependsOn: ["model"],
    ariaPatterns: ["trim"],
    optional: true,
  },
  {
    key: "price",
    labels: ["Price"],
    control: "text",
    ariaPatterns: ["price"],
  },
  {
    key: "mileage",
    labels: ["Mileage", "Odometer"],
    control: "text",
    ariaPatterns: ["mileage", "odometer"],
    optional: true,
  },
  {
    key: "body_style",
    labels: ["Body style", "Body Style"],
    control: "combobox",
    ariaPatterns: ["body style"],
    optional: true,
  },
  {
    key: "condition",
    labels: ["Vehicle condition", "Condition"],
    control: "combobox",
    ariaPatterns: ["condition"],
  },
  {
    key: "exterior_color",
    labels: ["Exterior color", "Exterior Color"],
    control: "combobox",
    ariaPatterns: ["exterior color"],
    optional: true,
  },
  {
    key: "interior_color",
    labels: ["Interior color", "Interior Color"],
    control: "combobox",
    ariaPatterns: ["interior color"],
    optional: true,
  },
  {
    key: "transmission",
    labels: ["Transmission"],
    control: "combobox",
    ariaPatterns: ["transmission"],
    optional: true,
  },
  {
    key: "drivetrain",
    labels: ["Drivetrain", "Drive type"],
    control: "combobox",
    ariaPatterns: ["drivetrain", "drive type"],
    optional: true,
  },
  {
    key: "fuel_type",
    labels: ["Fuel type", "Fuel Type"],
    control: "combobox",
    ariaPatterns: ["fuel type"],
    optional: true,
  },
  {
    key: "title",
    labels: ["Title"],
    control: "text",
    ariaPatterns: ["title"],
    optional: true,
  },
  {
    key: "description",
    labels: ["Description"],
    control: "textarea",
    cssFallbacks: ['textarea[aria-label*="Description" i]'],
  },
  {
    key: "location",
    labels: ["Location", "Meetup location"],
    control: "location",
    ariaPatterns: ["location"],
    optional: true,
  },
];

export const PHOTO_FIELD = {
  key: "photos",
  labels: ["Add photos", "Upload photos", "Photos"],
  cssFallbacks: [
    'input[type="file"][accept*="image" i]',
    'input[type="file"]',
  ],
};

/**
 * @param {string} key
 * @returns {FieldDefinition | undefined}
 */
export function fieldByKey(key) {
  return VEHICLE_CREATE_FIELDS.find((f) => f.key === key);
}
