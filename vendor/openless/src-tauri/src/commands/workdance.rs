//! Debug / test surface for the gesture gate.

use std::sync::Arc;

use tauri::State;

use crate::workdance_host::WorkdanceRuntime;
use workdance_gesture::{DecisionKind, EventCounts, GateSnapshot};

#[tauri::command]
pub fn workdance_gesture_set_enabled(
    enabled: bool,
    runtime: State<'_, Arc<WorkdanceRuntime>>,
) -> bool {
    runtime.set_enabled(enabled)
}

#[tauri::command]
pub fn workdance_gesture_gate_state(runtime: State<'_, Arc<WorkdanceRuntime>>) -> GateSnapshot {
    runtime.status().gate
}

#[tauri::command]
pub fn workdance_gesture_last_decision(runtime: State<'_, Arc<WorkdanceRuntime>>) -> DecisionKind {
    runtime.status().gate.last_decision
}

#[tauri::command]
pub fn workdance_gesture_event_counts(runtime: State<'_, Arc<WorkdanceRuntime>>) -> EventCounts {
    runtime.status().gate.counts
}
