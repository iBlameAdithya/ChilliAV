//! chilli-core: Core agent execution state machine and lifecycle engine.

use std::sync::atomic::AtomicBool;

pub static ABLATION_VERIFICATION: AtomicBool = AtomicBool::new(true);
pub static ABLATION_DIAGNOSTIC_SLICING: AtomicBool = AtomicBool::new(true);
pub static ABLATION_ADAPTIVE_POLICY: AtomicBool = AtomicBool::new(true);
pub static ABLATION_CODEGRAPH: AtomicBool = AtomicBool::new(true);

pub fn set_ablation_settings(
    hysteresis: bool,
    verification: bool,
    diagnostic_slicing: bool,
    adaptive_policy: bool,
    codegraph: bool,
) {
    chilli_model::capabilities::ENABLE_HYSTERESIS
        .store(hysteresis, std::sync::atomic::Ordering::SeqCst);
    ABLATION_VERIFICATION.store(verification, std::sync::atomic::Ordering::SeqCst);
    ABLATION_DIAGNOSTIC_SLICING.store(diagnostic_slicing, std::sync::atomic::Ordering::SeqCst);
    ABLATION_ADAPTIVE_POLICY.store(adaptive_policy, std::sync::atomic::Ordering::SeqCst);
    ABLATION_CODEGRAPH.store(codegraph, std::sync::atomic::Ordering::SeqCst);
}

pub mod agent;
pub mod engine;
pub mod loop_detector;
pub mod state;
pub mod stream_accumulator;
pub mod verifier;
