//! Request-boundary account FSM, orthogonal to [`RetryGate`] (RFC §4).
//!
//! Switch is not folded into `RetryGate.max_attempts`. Grok encrypted-reasoning
//! strip is a same-account recovery and is not modeled here.

use super::types::{EmissionState, IrEvent, RetryClass, RetryGate};

#[cfg(test)]
mod tests;

/// One switch per request, only while output is still Idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccountSwitchGate {
    used: bool,
}

impl AccountSwitchGate {
    pub fn used(self) -> bool {
        self.used
    }

    pub fn can_switch(
        self,
        emission: EmissionState,
        multi_account: bool,
        has_failover: bool,
        class: SwitchClass,
    ) -> bool {
        !self.used
            && emission == EmissionState::Idle
            && multi_account
            && has_failover
            && class == SwitchClass::AccountFailure
    }

    pub fn mark_used(&mut self) {
        self.used = true;
    }
}

/// Whether this failure is an account-level fault that may failover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchClass {
    /// NeedsLogin / persistent 401 after reload / health probe fail.
    AccountFailure,
    /// 429, 5xx, protocol 400, Grok reasoning decode — stay on this account.
    NotAccountFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestDecision {
    /// Same-account OAuth follow/refresh (RetryGate). Caller must reload.
    ReloadSameAccount,
    /// Isolate the current member and pick the next eligible one.
    SwitchAccount,
    /// Map to the existing upstream_error path. Do not switch.
    Fail,
}

/// Per-request gates. Worst path: A reload → switch B → B reload.
#[derive(Debug, Clone)]
pub struct RequestFsm {
    pub emission: EmissionState,
    retry: RetryGate,
    retry_used: bool,
    switch: AccountSwitchGate,
    pub multi_account: bool,
}

impl RequestFsm {
    pub fn new(multi_account: bool) -> Self {
        Self {
            emission: EmissionState::Idle,
            retry: RetryGate::default(),
            retry_used: false,
            switch: AccountSwitchGate::default(),
            multi_account,
        }
    }

    pub fn retry_used(&self) -> bool {
        self.retry_used
    }

    pub fn switch_used(&self) -> bool {
        self.switch.used()
    }

    pub fn observe(&mut self, event: &IrEvent) {
        self.emission = self.emission.observe(event);
    }

    pub fn mark_emitted(&mut self) {
        self.emission = EmissionState::Emitted;
    }

    /// Classify a finished non-success upstream attempt.
    ///
    /// `oauth_401` is true only for OAuth protocols that may follow/refresh.
    /// API Key 401 is an account failure but never a RetryGate reload.
    pub fn on_failure(
        &self,
        oauth_401: bool,
        class: SwitchClass,
        has_failover: bool,
    ) -> RequestDecision {
        if self.emission != EmissionState::Idle {
            return RequestDecision::Fail;
        }
        if oauth_401
            && !self.retry_used
            && self
                .retry
                .can_retry(self.emission, RetryClass::Transient, 0)
        {
            return RequestDecision::ReloadSameAccount;
        }
        if self
            .switch
            .can_switch(self.emission, self.multi_account, has_failover, class)
        {
            return RequestDecision::SwitchAccount;
        }
        RequestDecision::Fail
    }

    pub fn record_retry(&mut self) {
        self.retry_used = true;
    }

    /// After a successful switch the new account gets its own 401 reload.
    pub fn record_switch(&mut self) {
        self.switch.mark_used();
        self.retry_used = false;
    }
}
