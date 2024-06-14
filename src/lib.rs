//! Structures and tools to parse, navigate and validate [OpenAPI v3.1] specifications.
//!
//! # Example
//!
//! ```no_run
//! match oas3::from_path("path/to/openapi.yaml") {
//!   Ok(spec) => println!("spec: {:?}", spec),
//!   Err(err) => println!("error: {}", err)
//! }
//! ```
//!
//! [OpenAPI v3.1]: https://github.com/OAI/OpenAPI-Specification/blob/HEAD/versions/3.1.0.md

#![deny(rust_2018_idioms, nonstandard_style)]
#![warn(missing_debug_implementations)]

use serde::de::{Deserializer, MapAccess, Visitor};
use std::fmt;
use std::{fs::File, io::Read, path::Path};

mod error;
pub mod spec;

pub use error::Error;
pub use spec::{Schema, Spec};

#[cfg(feature = "validation")]
pub mod validation;

#[cfg(feature = "conformance")]
pub mod conformance;

/// Version 3.1.0 of the OpenAPI specification.
///
/// Refer to the official [specification] for more information.
///
/// [specification]: https://github.com/OAI/OpenAPI-Specification/blob/HEAD/versions/3.1.0.md
pub type OpenApiV3Spec = spec::Spec;

/// Try deserializing an OpenAPI spec (YAML or JSON) from a file, giving the path.
pub fn from_path<P>(path: P) -> Result<OpenApiV3Spec, Error>
where
    P: AsRef<Path>,
{
    from_reader(File::open(path)?)
}

/// Try deserializing an OpenAPI spec (YAML or JSON) from a [`Read`] type.
pub fn from_reader<R>(read: R) -> Result<OpenApiV3Spec, Error>
where
    R: Read,
{
    Ok(serde_yaml::from_reader::<R, OpenApiV3Spec>(read)?)
}

/// Try serializing to a YAML string.
pub fn to_yaml(spec: &OpenApiV3Spec) -> Result<String, Error> {
    Ok(serde_yaml::to_string(spec)?)
}

/// Try serializing to a JSON string.
pub fn to_json(spec: &OpenApiV3Spec) -> Result<String, Error> {
    Ok(serde_json::to_string_pretty(spec)?)
}

pub fn deserialize_extensions<'de, D>(deserializer: D) -> Result<serde_yaml::Value, D::Error>
where
    D: Deserializer<'de>,
{
    struct ExtraFieldsVisitor;

    impl<'de> Visitor<'de> for ExtraFieldsVisitor {
        type Value = serde_yaml::Value;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("extensions")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut map = serde_yaml::Mapping::new();
            while let Some((key, value)) = access.next_entry()? {
                map.insert(key, value);
            }

            Ok(map.into())
        }
    }

    deserializer.deserialize_map(ExtraFieldsVisitor)
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, read_to_string, File},
        io::Write,
        path,
    };

    use crate::spec::{Components, ObjectOrReference};
    use pretty_assertions::assert_eq;

    use super::*;

    /// Helper function to write string to file.
    fn write_to_file<P>(path: P, filename: &str, data: &str)
    where
        P: AsRef<Path> + std::fmt::Debug,
    {
        println!("    Saving string to {:?}...", path);
        std::fs::create_dir_all(&path).unwrap();
        let full_filename = path.as_ref().to_path_buf().join(filename);
        let mut f = File::create(full_filename).unwrap();
        f.write_all(data.as_bytes()).unwrap();
    }

    /// Convert a YAML `&str` to a JSON `String`.
    fn convert_yaml_str_to_json(yaml_str: &str) -> String {
        let yaml: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let json: serde_json::Value = serde_yaml::from_value(yaml).unwrap();
        serde_json::to_string_pretty(&json).unwrap()
    }

    /// Deserialize and re-serialize the input file to a JSON string through two different
    /// paths, comparing the result.
    /// 1. File -> `String` -> `serde_yaml::Value` -> `serde_json::Value` -> `String`
    /// 2. File -> `Spec` -> `serde_json::Value` -> `String`
    ///
    /// Both conversion of `serde_json::Value` -> `String` are done
    /// using `serde_json::to_string_pretty`.
    /// Since the first conversion is independent of the current crate (and only
    /// uses serde json and yaml support), no information should be lost in the final
    /// JSON string. The second conversion goes through our `OpenApi`, so the final JSON
    /// string is a representation of _our_ implementation.
    /// By comparing those two JSON conversions, we can validate our implementation.
    fn compare_spec_through_json(
        input_file: &Path,
        save_path_base: &Path,
    ) -> (String, String, String) {
        // First conversion:
        //     File -> `String` -> `serde_yaml::Value` -> `serde_json::Value` -> `String`

        // Read the original file to string
        let spec_yaml_str = read_to_string(input_file)
            .unwrap_or_else(|e| panic!("failed to read contents of {:?}: {}", input_file, e));
        // Convert YAML string to JSON string
        let spec_json_str = convert_yaml_str_to_json(&spec_yaml_str);

        // Second conversion:
        //     File -> `Spec` -> `serde_json::Value` -> `String`

        // Parse the input file
        let parsed_spec = from_path(input_file).unwrap();
        // Convert to serde_json::Value
        let parsed_spec_json = serde_json::to_value(parsed_spec).unwrap();
        // Convert to a JSON string
        let parsed_spec_json_str: String = serde_json::to_string_pretty(&parsed_spec_json).unwrap();

        // Save JSON strings to file
        let api_filename = input_file
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(".yaml", ".json");

        let mut save_path = save_path_base.to_path_buf();
        save_path.push("yaml_to_json");
        write_to_file(&save_path, &api_filename, &spec_json_str);

        let mut save_path = save_path_base.to_path_buf();
        save_path.push("yaml_to_spec_to_json");
        write_to_file(&save_path, &api_filename, &parsed_spec_json_str);

        // Return the JSON filename and the two JSON strings
        (api_filename, parsed_spec_json_str, spec_json_str)
    }

    #[test]
    #[ignore = "lib does not support all schema attributes yet"]
    fn test_serialization_round_trip() {
        let save_path_base: path::PathBuf = ["target", "tests", "test_serialization_round_trip"]
            .iter()
            .collect();

        for entry in fs::read_dir("data/oas-samples").unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();

            println!("Testing if {:?} is deserializable", path);

            let (api_filename, parsed_spec_json_str, spec_json_str) =
                compare_spec_through_json(&path, &save_path_base);

            assert_eq!(
                parsed_spec_json_str.lines().collect::<Vec<_>>(),
                spec_json_str.lines().collect::<Vec<_>>(),
                "contents did not match for api {}",
                api_filename
            );
        }
    }

    #[test]
    fn test_json_from_reader() {
        let yaml = r#"openapi: "3"
paths: {}
info:
  title: Test API
  version: "0.1"
components:
  schemas:
    assets:
      title: Assets
      type: array
      items: { type: integer }"#;

        let json = r#"{
  "openapi": "3",
  "paths": {},
  "info": {
    "title": "Test API",
    "version": "0.1"
  },
  "components": {
    "schemas": {
      "assets": {
        "title": "Assets",
        "type": "array",
        "items": {
          "type": "integer"
        }
      }
    }
  }
}"#;

        assert_eq!(
            from_reader(json.as_bytes()).unwrap(),
            from_reader(yaml.as_bytes()).unwrap()
        );
    }
}
