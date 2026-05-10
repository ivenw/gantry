use std::future::Future;

use rig::completion::CompletionModel;
use rig::wasm_compat::WasmCompatSend;
use tokio::sync::mpsc::UnboundedSender;

/// An event emitted by [`PromptHook`] during agent prompt execution.
#[derive(Debug, Clone)]
pub enum HookEvent {
    /// Fired immediately before a tool is executed.
    ToolCallStarted { name: String, id: String },
    /// Fired immediately after a tool returns its result.
    ToolCallFinished { id: String },
}

/// A [`rig::agent::PromptHook`] that forwards prompt lifecycle events over a channel.
#[derive(Clone)]
pub struct PromptHook {
    tx: UnboundedSender<HookEvent>,
}

impl PromptHook {
    /// Creates a new hook that sends events on `tx`.
    pub fn new(tx: UnboundedSender<HookEvent>) -> Self {
        Self { tx }
    }
}

impl<M: CompletionModel> rig::agent::PromptHook<M> for PromptHook {
    fn on_tool_call(
        &self,
        tool_name: &str,
        _tool_call_id: Option<String>,
        internal_call_id: &str,
        _args: &str,
    ) -> impl Future<Output = rig::agent::ToolCallHookAction> + WasmCompatSend {
        let _ = self.tx.send(HookEvent::ToolCallStarted {
            name: tool_name.to_string(),
            id: internal_call_id.to_string(),
        });
        async { rig::agent::ToolCallHookAction::cont() }
    }

    fn on_tool_result(
        &self,
        _tool_name: &str,
        _tool_call_id: Option<String>,
        internal_call_id: &str,
        _args: &str,
        _result: &str,
    ) -> impl Future<Output = rig::agent::HookAction> + WasmCompatSend {
        let _ = self.tx.send(HookEvent::ToolCallFinished {
            id: internal_call_id.to_string(),
        });
        async { rig::agent::HookAction::cont() }
    }
}
