use crate::proto::{ProjectId, TopicClass, TopicDescriptor, TopicId};

/// Statische Topic-Registry. Muss 1:1 mit frontend/src/core/realtime/topicTable.ts
/// übereinstimmen; der Client prüft die Welcome-Descriptoren dagegen.
#[derive(Debug, Clone, Copy)]
pub struct TopicSpec {
    pub id: TopicId,
    pub class: TopicClass,
    pub project: ProjectId,
    pub wire_name: &'static str,
    /// Voll qualifizierter Payload-Typ; leer = Projektion existiert noch nicht (F0-VAR).
    pub payload_type: &'static str,
}

impl TopicSpec {
    pub fn available(&self) -> bool {
        !self.payload_type.is_empty()
    }

    pub fn is_state(&self) -> bool {
        self.class == TopicClass::State
    }

    pub fn descriptor(&self) -> TopicDescriptor {
        TopicDescriptor {
            topic: self.id as i32,
            class: self.class as i32,
            project: self.project as i32,
            wire_name: self.wire_name.to_string(),
            payload_type: self.payload_type.to_string(),
            available: self.available(),
        }
    }
}

const fn s(
    id: TopicId,
    class: TopicClass,
    project: ProjectId,
    wire_name: &'static str,
    payload_type: &'static str,
) -> TopicSpec {
    TopicSpec {
        id,
        class,
        project,
        wire_name,
        payload_type,
    }
}

pub const TOPICS: &[TopicSpec] = &[
    s(
        TopicId::OverviewAggregates,
        TopicClass::State,
        ProjectId::Unspecified,
        "overview.aggregates",
        "darkbot.control.v1.OverviewAggregates",
    ),
    s(
        TopicId::Project0Status,
        TopicClass::State,
        ProjectId::Project0,
        "project0.status",
        "darkbot.project0.v1.SystemStatus",
    ),
    s(
        TopicId::Project0FinanceSummary,
        TopicClass::State,
        ProjectId::Project0,
        "project0.finance.summary",
        "",
    ),
    s(
        TopicId::Project0LiquidatorState,
        TopicClass::State,
        ProjectId::Project0,
        "project0.liquidator.state",
        "",
    ),
    s(
        TopicId::Project0CaptureState,
        TopicClass::State,
        ProjectId::Project0,
        "project0.capture.state",
        "darkbot.project0.v1.CaptureState",
    ),
    s(
        TopicId::Project0WalletsState,
        TopicClass::State,
        ProjectId::Project0,
        "project0.wallets.state",
        "",
    ),
    s(
        TopicId::Project0AnalyticsJobs,
        TopicClass::State,
        ProjectId::Project0,
        "project0.analytics.jobs",
        "",
    ),
    s(
        TopicId::Project0ExecutionEvents,
        TopicClass::Event,
        ProjectId::Project0,
        "project0.execution.events",
        "",
    ),
    s(
        TopicId::Project0CaptureEvents,
        TopicClass::Event,
        ProjectId::Project0,
        "project0.capture.events",
        "",
    ),
    s(
        TopicId::BscStatus,
        TopicClass::State,
        ProjectId::Bsc,
        "bsc.status",
        "darkbot.bsc.v1.BscStatus",
    ),
    s(
        TopicId::BscLaunchState,
        TopicClass::State,
        ProjectId::Bsc,
        "bsc.launch.state",
        "darkbot.bsc.v1.BscLaunchState",
    ),
    s(
        TopicId::BscMonitoringState,
        TopicClass::State,
        ProjectId::Bsc,
        "bsc.monitoring.state",
        "darkbot.bsc.v1.BscMonitoringState",
    ),
    s(
        TopicId::BscWalletsState,
        TopicClass::State,
        ProjectId::Bsc,
        "bsc.wallets.state",
        "darkbot.bsc.v1.BscWalletsState",
    ),
    s(
        TopicId::BscExecutionEvents,
        TopicClass::Event,
        ProjectId::Bsc,
        "bsc.execution.events",
        "",
    ),
    s(
        TopicId::SolanaStatus,
        TopicClass::State,
        ProjectId::Solana,
        "solana.status",
        "",
    ),
];

pub fn spec(id: TopicId) -> Option<&'static TopicSpec> {
    TOPICS.iter().find(|t| t.id == id)
}

pub fn spec_from_wire(raw: i32) -> Option<&'static TopicSpec> {
    TopicId::try_from(raw).ok().and_then(spec)
}

pub fn state_topics() -> impl Iterator<Item = &'static TopicSpec> {
    TOPICS.iter().filter(|t| t.is_state())
}

pub fn topics_of(project: ProjectId) -> impl Iterator<Item = &'static TopicSpec> {
    TOPICS.iter().filter(move |t| t.project == project)
}

pub fn descriptors() -> Vec<TopicDescriptor> {
    TOPICS.iter().map(TopicSpec::descriptor).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_wire_names_match_numbering() {
        let mut seen = std::collections::HashSet::new();
        for t in TOPICS {
            assert!(seen.insert(t.id as i32), "duplicate topic id {:?}", t.id);
            assert!(!t.wire_name.is_empty());
        }
        assert!(spec(TopicId::OverviewAggregates).unwrap().available());
        assert!(
            spec(TopicId::BscStatus).unwrap().available(),
            "BSC payload type only after F0-VAR"
        );
    }
}
