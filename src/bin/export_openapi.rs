//! Export OpenAPI specification to static JSON file
//!
//! Usage: cargo run --bin export_openapi
//!
//! Generates docs/openapi.json for SDK generation and API linting.

use sentinel::docs::NativeApiDoc;
use std::fs;
use utoipa::OpenApi;

fn main() {
    let spec = NativeApiDoc::openapi();
    let json = spec.to_pretty_json().expect("Failed to serialize OpenAPI spec");

    // Ensure docs directory exists
    fs::create_dir_all("docs").expect("Failed to create docs directory");

    fs::write("docs/openapi.json", json).expect("Failed to write openapi.json");
    println!("Exported OpenAPI spec to docs/openapi.json");
}
