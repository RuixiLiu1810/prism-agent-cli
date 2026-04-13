// Agent core library — framework-agnostic agent runtime.

pub mod config;
pub mod events;
pub mod event_sink;
pub mod provider;

pub use config::{
    AgentDomainConfig, AgentRuntimeConfig, AgentSamplingConfig, AgentSamplingProfilesConfig,
    ConfigProvider, StaticConfigProvider,
};
pub use event_sink::{EventSink, NullEventSink};
pub use events::*;
pub use provider::{
    AgentProvider, AgentResponseMode, AgentSamplingProfile, AgentSelectionScope, AgentStatus,
    AgentTaskKind, AgentTurnDescriptor, AgentTurnHandle, AgentTurnProfile,
};
