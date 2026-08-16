//! Common registration helpers (detector + skill target).

use std::sync::Arc;

use crate::models::AgentId;
use crate::platform::detection::{AgentDetector, FnDetector};
use crate::platform::skills::StaticSkillTarget;
use crate::platform::AgentKey;

use super::super::IntegrationContext;

pub(crate) fn register_fn_detector(
    ctx: &mut IntegrationContext<'_>,
    agent: AgentId,
    observe: fn() -> crate::models::DetectResult,
) {
    let agent_key = AgentKey::from_agent_id(agent);
    ctx.detectors
        .register(
            Arc::new(FnDetector::new(agent_key, move || observe().into()))
                as Arc<dyn AgentDetector>,
        )
        .expect("unique built-in detector");
}

pub(crate) fn register_skills_from_home(
    ctx: &mut IntegrationContext<'_>,
    agent: AgentId,
    child: &str,
) {
    let Some(root) = ctx
        .paths
        .get(agent)
        .and_then(|p| p.home_dir().ok())
        .map(|home| home.join(child))
    else {
        return;
    };
    ctx.skills
        .register(Arc::new(StaticSkillTarget {
            agent_key: AgentKey::from_agent_id(agent),
            skills_root: Some(root),
            supports: true,
        }))
        .expect("unique built-in skill target");
}

pub(crate) fn register_skills_from_config_dir(
    ctx: &mut IntegrationContext<'_>,
    agent: AgentId,
    child: &str,
) {
    let Some(root) = ctx
        .paths
        .get(agent)
        .and_then(|p| p.config_dir().ok())
        .map(|dir| dir.join(child))
    else {
        return;
    };
    ctx.skills
        .register(Arc::new(StaticSkillTarget {
            agent_key: AgentKey::from_agent_id(agent),
            skills_root: Some(root),
            supports: true,
        }))
        .expect("unique built-in skill target");
}
