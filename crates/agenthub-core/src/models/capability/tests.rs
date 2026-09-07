use super::*;

#[test]
fn all_covers_every_variant() {
    assert_eq!(Capability::ALL.len(), 14);
    // Exhaustive: adding a variant without updating ALL fails this match.
    for cap in Capability::ALL {
        let _ = match cap {
            Capability::ConfigWrite
            | Capability::AccountSwitch
            | Capability::ApiKeyAccount
            | Capability::Skills
            | Capability::LiveBackup
            | Capability::StructuredStream
            | Capability::DangerousMode
            | Capability::ProjectHistory
            | Capability::ProjectDelete
            | Capability::ProviderPresets
            | Capability::Usage
            | Capability::Mcp
            | Capability::ModelSelect
            | Capability::SessionResume => cap.as_str(),
        };
    }
}

#[test]
fn blocked_and_usable_partition() {
    assert!(CapabilityState::full().is_usable());
    assert!(!CapabilityState::full().is_blocked());
    assert!(CapabilityState::partial("x").is_usable());
    assert!(!CapabilityState::partial("x").is_blocked());
    assert!(CapabilityState::unsupported("x").is_blocked());
    assert!(!CapabilityState::unsupported("x").is_usable());
    assert!(CapabilityState::planned("x").is_blocked());
    assert!(!CapabilityState::planned("x").is_usable());
}

#[test]
fn serde_camel_case() {
    let json = serde_json::to_string(&Capability::AccountSwitch).unwrap();
    assert_eq!(json, "\"accountSwitch\"");
    let level = serde_json::to_string(&CapabilityLevel::Unsupported).unwrap();
    assert_eq!(level, "\"unsupported\"");
}

#[test]
fn dto_roundtrip_owns_reason_strings() {
    let state = CapabilityState::partial("降级说明");
    let dto = CapabilityStateDto::from(state);
    assert_eq!(dto.level, CapabilityLevel::Partial);
    assert_eq!(dto.reason.as_deref(), Some("降级说明"));
    let json = serde_json::to_value(&dto).unwrap();
    assert_eq!(json["level"], "partial");
    assert_eq!(json["reason"], "降级说明");
    let back: CapabilityStateDto = serde_json::from_value(json).unwrap();
    assert_eq!(back, dto);
}

#[test]
fn label_covers_all_capabilities() {
    for cap in Capability::ALL {
        assert!(!cap.label().is_empty(), "{cap:?}");
        assert!(!cap.as_str().is_empty(), "{cap:?}");
    }
}
