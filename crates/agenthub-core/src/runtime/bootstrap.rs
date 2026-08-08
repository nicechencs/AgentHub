//! Remediation plans for missing runtimes (no auto-install in P0).

use crate::models::{Remediation, RuntimeId};

pub fn remediation_for(id: RuntimeId) -> Remediation {
    match id {
        RuntimeId::NodeJs => Remediation {
            kind: "winget".into(),
            command: Some("winget install OpenJS.NodeJS.LTS".into()),
            url: Some("https://nodejs.org/".into()),
            text: Some(
                "Install Node.js LTS, then restart the shell / AgentHub so PATH refreshes.".into(),
            ),
        },
        RuntimeId::Npm => Remediation {
            kind: "hint".into(),
            command: None,
            url: Some("https://nodejs.org/".into()),
            text: Some(
                "npm usually ships with Node.js. If node works but npm does not, repair PATH or reinstall Node."
                    .into(),
            ),
        },
        RuntimeId::PowerShell => Remediation {
            kind: "hint".into(),
            command: None,
            url: Some("https://learn.microsoft.com/powershell/scripting/install/installing-powershell".into()),
            text: Some(
                "Windows usually ships PowerShell 5.1 (System32). PowerShell 7 (pwsh) is optional but preferred for native install scripts. AgentHub does not auto-install PowerShell; install pwsh manually if needed. Check ExecutionPolicy if scripts fail."
                    .into(),
            ),
        },
        RuntimeId::Git => Remediation {
            kind: "winget".into(),
            command: Some("winget install --id Git.Git -e --source winget".into()),
            url: Some("https://git-scm.com/downloads".into()),
            text: Some(
                "Install Git, then fully quit and restart AgentHub so PATH refreshes. Skills market / git URL install need git clone."
                    .into(),
            ),
        },
    }
}
