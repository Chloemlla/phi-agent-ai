use std::sync::{Arc, RwLock};

use super::auxiliary::{AuxiliaryPort, DecisionPort, ImagePort};
use super::memory::MemoryPort;
use super::ports::{
    ApproverPort, ContextPort, ModelPort, PolicyPort, RuntimePort, SandboxPort, SessionPort,
    ToolPort,
};

/// Typed service handles, wired by the composition root.
///
/// Plugins look each other up here by *contract*, never by concrete plugin
/// type (module-map rule R2). Missing optional services return `None`; callers
/// must degrade explicitly.
#[derive(Default)]
pub struct Services {
    session: RwLock<Option<Arc<dyn SessionPort>>>,
    model: RwLock<Option<Arc<dyn ModelPort>>>,
    policy: RwLock<Option<Arc<dyn PolicyPort>>>,
    sandbox: RwLock<Option<Arc<dyn SandboxPort>>>,
    approver: RwLock<Option<Arc<dyn ApproverPort>>>,
    context: RwLock<Option<Arc<dyn ContextPort>>>,
    runtime: RwLock<Option<Arc<dyn RuntimePort>>>,
    memory: RwLock<Option<Arc<dyn MemoryPort>>>,
    image: RwLock<Option<Arc<dyn ImagePort>>>,
    decision: RwLock<Option<Arc<dyn DecisionPort>>>,
    auxiliary: RwLock<Option<Arc<dyn AuxiliaryPort>>>,
    tools: RwLock<Vec<Arc<dyn ToolPort>>>,
}

impl Services {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_session(&self, value: Arc<dyn SessionPort>) {
        *self.session.write().unwrap() = Some(value);
    }
    pub fn session(&self) -> Option<Arc<dyn SessionPort>> {
        self.session.read().unwrap().clone()
    }

    pub fn set_model(&self, value: Arc<dyn ModelPort>) {
        *self.model.write().unwrap() = Some(value);
    }
    pub fn model(&self) -> Option<Arc<dyn ModelPort>> {
        self.model.read().unwrap().clone()
    }

    pub fn set_policy(&self, value: Arc<dyn PolicyPort>) {
        *self.policy.write().unwrap() = Some(value);
    }
    pub fn policy(&self) -> Option<Arc<dyn PolicyPort>> {
        self.policy.read().unwrap().clone()
    }

    pub fn set_sandbox(&self, value: Arc<dyn SandboxPort>) {
        *self.sandbox.write().unwrap() = Some(value);
    }
    pub fn sandbox(&self) -> Option<Arc<dyn SandboxPort>> {
        self.sandbox.read().unwrap().clone()
    }

    pub fn set_approver(&self, value: Arc<dyn ApproverPort>) {
        *self.approver.write().unwrap() = Some(value);
    }
    pub fn approver(&self) -> Option<Arc<dyn ApproverPort>> {
        self.approver.read().unwrap().clone()
    }

    pub fn set_context(&self, value: Arc<dyn ContextPort>) {
        *self.context.write().unwrap() = Some(value);
    }
    pub fn context(&self) -> Option<Arc<dyn ContextPort>> {
        self.context.read().unwrap().clone()
    }

    pub fn add_tool_provider(&self, value: Arc<dyn ToolPort>) {
        self.tools.write().unwrap().push(value);
    }
    pub fn runtime(&self) -> Option<Arc<dyn RuntimePort>> {
        self.runtime.read().unwrap().clone()
    }
    pub fn set_runtime(&self, value: Arc<dyn RuntimePort>) {
        *self.runtime.write().unwrap() = Some(value);
    }
    pub fn memory(&self) -> Option<Arc<dyn MemoryPort>> {
        self.memory.read().unwrap().clone()
    }
    pub fn set_memory(&self, value: Arc<dyn MemoryPort>) {
        *self.memory.write().unwrap() = Some(value);
    }
    pub fn image(&self) -> Option<Arc<dyn ImagePort>> {
        self.image.read().unwrap().clone()
    }
    pub fn set_image(&self, value: Arc<dyn ImagePort>) {
        *self.image.write().unwrap() = Some(value);
    }
    pub fn decision(&self) -> Option<Arc<dyn DecisionPort>> {
        self.decision.read().unwrap().clone()
    }
    pub fn set_decision(&self, value: Arc<dyn DecisionPort>) {
        *self.decision.write().unwrap() = Some(value);
    }
    pub fn auxiliary(&self) -> Option<Arc<dyn AuxiliaryPort>> {
        self.auxiliary.read().unwrap().clone()
    }
    pub fn set_auxiliary(&self, value: Arc<dyn AuxiliaryPort>) {
        *self.auxiliary.write().unwrap() = Some(value);
    }
    pub fn tool_providers(&self) -> Vec<Arc<dyn ToolPort>> {
        self.tools.read().unwrap().clone()
    }
    pub fn tool_definitions(&self) -> Vec<super::ports::ToolDefinition> {
        self.tools
            .read()
            .unwrap()
            .iter()
            .flat_map(|t| t.definitions())
            .collect()
    }
    pub fn tool_definition(&self, tool_id: &str) -> Option<super::ports::ToolDefinition> {
        self.find_tool(tool_id)
            .and_then(|tool| tool.definitions().into_iter().find(|d| d.id == tool_id))
    }
    pub fn find_tool(&self, tool_id: &str) -> Option<Arc<dyn ToolPort>> {
        self.tools
            .read()
            .unwrap()
            .iter()
            .find(|t| t.supports(tool_id))
            .cloned()
    }

    /// Check required slots against the typed wiring rather than trusting slot
    /// metadata alone. `persistence.journal` is owned by the kernel registry
    /// because the journal is a mechanism port, so it always counts as present
    /// here.
    pub fn missing_slots(&self, required: &[&str]) -> Vec<String> {
        required
            .iter()
            .filter(|slot| {
                if **slot == crate::SLOT_PERSISTENCE_JOURNAL {
                    return false;
                }
                SERVICE_SLOTS
                    .iter()
                    .find(|entry| entry.0 == **slot)
                    .map(|(_, present)| !present(self))
                    .unwrap_or(true)
            })
            .map(|slot| (*slot).to_string())
            .collect()
    }
}

/// Single mapping from a slot name to the typed handle that satisfies it.
/// Adding a required service means adding one row here instead of editing
/// `Services` and `Node` separately.
const SERVICE_SLOTS: &[(&str, fn(&Services) -> bool)] = &[
    (crate::SLOT_RUNTIME_SKELETON, |services| {
        services.runtime().is_some()
    }),
    (crate::SLOT_SESSION_STORE, |services| {
        services.session().is_some()
    }),
    (crate::SLOT_POLICY_EVALUATE, |services| {
        services.policy().is_some()
    }),
    (crate::SLOT_SANDBOX_EXECUTE, |services| {
        services.sandbox().is_some()
    }),
    (crate::SLOT_APPROVAL_DECIDE, |services| {
        services.approver().is_some()
    }),
    (crate::SLOT_MODEL_INVOKE, |services| {
        services.model().is_some()
    }),
    (crate::SLOT_MEMORY_STORE, |services| {
        services.memory().is_some()
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_services_report_missing_required_slots() {
        let services = Services::new();
        let missing = services.missing_slots(crate::REQUIRED_SLOTS);
        assert!(missing.contains(&crate::SLOT_RUNTIME_SKELETON.to_string()));
        assert!(missing.contains(&crate::SLOT_MODEL_INVOKE.to_string()));
        assert!(!missing.contains(&crate::SLOT_PERSISTENCE_JOURNAL.to_string()));
    }
}
