//! P6-MT.10 phase 2: the local fee model as a capability of the one controller
//! (`LOCAL_FEE_MODEL` OFF|OBSERVE|ACTIVE, start value `P0_LOCAL_FEE_MODEL`). ACTIVE decides a
//! single transaction's fee without an RPC quote only in a class the project's newest seal of the
//! saved runs qualifies (`fee_parity::sealed_classes`, read at the start and on each request) and
//! no mismatch of this run failed; the simulation verifies each decided fee before anything is
//! sent. A mismatch fails its class for good (the RPC quote again, the status FAIL); a simulation
//! without the node's fee ends ACTIVE for the run (nothing verifies the model with that
//! provider). OBSERVE is phase 1's behaviour, OFF computes nothing.
use super::ShadowOwner;
use crate::execution::fee_parity::{self, ClassSet, FeeModel, FeeModelMode, Unverified};

/// The owner's local fee model.
#[derive(Debug)]
pub(super) struct LocalFeeControl {
    pub(super) requested: FeeModelMode,
    /// The project the seal is read from (`None`: no project, never sealed).
    pub(super) project: Option<std::path::PathBuf>,
    /// The seal's qualified classes as last read, or why none.
    pub(super) sealed: Result<ClassSet, String>,
    /// Why ACTIVE ended for this run: a simulation verified nothing.
    pub(super) unverified: Option<Unverified>,
}

impl LocalFeeControl {
    /// The start value and the project's seal as it is now (the owner's own run excluded).
    pub(super) fn start(
        requested: FeeModelMode,
        project: Option<std::path::PathBuf>,
        run_id: &str,
    ) -> LocalFeeControl {
        let sealed = sealed(project.as_deref(), run_id);
        LocalFeeControl {
            requested,
            project,
            sealed,
            unverified: None,
        }
    }

    /// After an evaluation: a decided fee its simulation could not verify ends ACTIVE for this
    /// run (a mismatch fails only its class, through the tally).
    pub(super) fn after(&mut self, reason: Option<&crate::analytics::rejection::Rejection>) {
        if let Some(crate::analytics::rejection::Rejection::LocalFee(
            e @ (Unverified::NoNodeFee | Unverified::OtherMessage),
        )) = reason
        {
            self.unverified.get_or_insert(*e);
        }
    }
}

fn sealed(project: Option<&std::path::Path>, run_id: &str) -> Result<ClassSet, String> {
    match project {
        Some(p) => fee_parity::sealed_classes(p, run_id),
        None => Err("LOCAL_FEE_NOT_SEALED: no project (P0_PROJECT_DIR)".into()),
    }
}

impl ShadowOwner {
    /// What this evaluation does with the local model: ACTIVE in the sealed classes no mismatch
    /// of this run failed, while every decided fee so far was verified.
    pub(super) fn fee_model(&self) -> FeeModel {
        let c = &self.local_fee;
        match (c.requested, &c.sealed, c.unverified) {
            (FeeModelMode::Off, _, _) => FeeModel::OFF,
            (FeeModelMode::Active, Ok(classes), None) => FeeModel::active(
                classes.without(self.fee_parity.failed(self.fee_parity_base.as_ref())),
            ),
            _ => FeeModel::OBSERVE,
        }
    }

    /// A controller request (`MODE_VALUES`): applied at once; the seal is read again, so a seal
    /// made while the owner runs counts from the next request on.
    pub(super) fn local_fee_request(&mut self, requested: &str) -> Result<String, String> {
        let mode = FeeModelMode::parse(requested)
            .ok_or_else(|| format!("LOCAL_FEE_MODEL_INVALID: {requested} (OFF|OBSERVE|ACTIVE)"))?;
        self.local_fee.requested = mode;
        self.local_fee.sealed = sealed(self.local_fee.project.as_deref(), &self.run_id);
        Ok(requested.to_string())
    }

    /// The request, what is effective and why not as requested — with this run's parity.
    pub(super) fn local_fee_state(&self) -> (&'static str, &'static str, String) {
        let c = &self.local_fee;
        let model = self.fee_model();
        let effective = if model.classes().is_empty() {
            c.requested.min(FeeModelMode::Observe)
        } else {
            FeeModelMode::Active
        };
        let run = fee_parity::SavedRuns {
            runs: Vec::new(),
            records: vec![self.fee_parity.record(self.fee_parity_base.as_ref())],
            crashed: Vec::new(),
            unreadable: Vec::new(),
        };
        let parity = fee_parity::status_line(&fee_parity::qualification(&run), "this run");
        let why = match (c.requested, &c.sealed, c.unverified) {
            (FeeModelMode::Active, _, Some(e)) => format!(
                "; ACTIVE ended: {}",
                crate::analytics::rejection::Rejection::LocalFee(e).text()
            ),
            (FeeModelMode::Active, Err(reason), None) => format!("; {reason}"),
            (FeeModelMode::Active, Ok(sealed), None)
                if FeeModel::active(*sealed).classes().is_empty() =>
            {
                "; LOCAL_FEE_NO_SINGLE_CLASS: the seal qualifies no single-transaction class (a bundle part is always quoted)".into()
            }
            (FeeModelMode::Active, Ok(_), None) if model.classes().is_empty() => {
                "; LOCAL_FEE_CLASSES_FAILED: every sealed class failed in this run".into()
            }
            _ if !model.classes().is_empty() => format!(
                "; the local model decides in {} sealed classes, the RPC quote in the others",
                model.classes().len()
            ),
            _ => "; the RPC quote decides".into(),
        };
        (
            c.requested.name(),
            effective.name(),
            format!("{parity}{why}"),
        )
    }
}
