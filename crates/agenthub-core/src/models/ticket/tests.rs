use super::*;
use serde_json::Value;

const FIXTURE: &str = include_str!("../../../../../src/lib/backend/contracts/ticket-speaks.json");

fn all_surfaces() -> [TicketSurface; 10] {
    [
        TicketSurface::KimiCodeMembership,
        TicketSurface::AnthropicApi,
        TicketSurface::OpenaiApi,
        TicketSurface::XaiApi,
        TicketSurface::GlmCodingPlan,
        TicketSurface::DeepseekApi,
        TicketSurface::CodexChatgptSubscription,
        TicketSurface::ClaudeSubscription,
        TicketSurface::GrokXaiSubscription,
        TicketSurface::Unknown,
    ]
}

#[test]
fn speaks_matches_shared_frontend_fixture() {
    let table: Value = serde_json::from_str(FIXTURE).expect("ticket-speaks.json");
    let object = table.as_object().expect("speaks table is an object");
    assert_eq!(object.len(), all_surfaces().len());

    for surface in all_surfaces() {
        let expected = object
            .get(surface.as_str())
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("missing speaks for {}", surface.as_str()));
        let got: Vec<&str> = surface.speaks().iter().map(|p| p.as_str()).collect();
        let want: Vec<&str> = expected
            .iter()
            .map(|v| v.as_str().expect("protocol is a string"))
            .collect();
        assert_eq!(got, want, "{}", surface.as_str());
    }
}
