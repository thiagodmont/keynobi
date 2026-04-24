use keynobi_lib::models::settings::AppSettings;

#[test]
fn app_settings_default_has_safe_values() {
    let settings = AppSettings::default();
    assert!(
        !settings.telemetry.enabled,
        "telemetry must default to false"
    );
    assert!(
        !settings.onboarding_completed,
        "fresh install must not have onboarding completed"
    );
    assert!(settings.appearance.ui_font_size > 0);
}

#[test]
fn app_settings_round_trips_through_json() {
    let original = AppSettings::default();
    let json = serde_json::to_string(&original).expect("AppSettings must serialize to JSON");
    let restored: AppSettings =
        serde_json::from_str(&json).expect("JSON must deserialize back to AppSettings");

    assert_eq!(original.onboarding_completed, restored.onboarding_completed);
    assert_eq!(
        original.appearance.ui_font_size,
        restored.appearance.ui_font_size
    );
}

#[test]
fn app_settings_json_uses_camel_case_keys() {
    let settings = AppSettings::default();
    let json = serde_json::to_string(&settings).expect("AppSettings should serialize");
    assert!(
        json.contains("\"onboardingCompleted\""),
        "key must be camelCase for TypeScript bindings"
    );
    assert!(json.contains("\"uiFontSize\""));
}
