use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A single discoverable Gradle build variant.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BuildVariant {
    /// Variant name as used in Gradle tasks (e.g. `freeDebug`, `release`).
    pub name: String,
    /// Build type component (e.g. `debug`, `release`).
    pub build_type: String,
    /// Product flavor components, if any (e.g. `["free"]`).
    pub flavors: Vec<String>,
    /// Gradle assemble task (e.g. `assembleFreeDebug`).
    pub assemble_task: String,
    /// Gradle install task (e.g. `installFreeDebug`).
    pub install_task: String,
}

/// The full list of variants discovered for the project.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
#[derive(Default)]
pub struct VariantList {
    pub variants: Vec<BuildVariant>,
    /// Currently selected variant name.
    pub active: Option<String>,
    /// Gradle / IDE–aligned default when nothing is persisted (e.g. `isDefault` flavors).
    pub default_variant: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_variant_serializes() {
        let v = BuildVariant {
            name: "freeDebug".into(),
            build_type: "debug".into(),
            flavors: vec!["free".into()],
            assemble_task: "assembleFreeDebug".into(),
            install_task: "installFreeDebug".into(),
        };
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("freeDebug"));
        assert!(json.contains("assembleFreeDebug"));
    }
}
