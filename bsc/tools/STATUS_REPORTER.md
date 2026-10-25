# Read-only status package

The reporter reads public DBREC and explicitly selected archived builder observations. It never executes a chain operation. Runtime readiness stays UNKNOWN; G0.7 readiness is independent.

`ops/darkbotctl bsc sync` (from the original repository terminal) imports completed GUI results byte-for-byte and stages a new immutable source snapshot. Both locations retain `bsc/native/evidence/g07/gui_readiness_<UUID>/`. `provenance.dbrec` (typed record, sealed and checked by `bsc-status-reporter`; BSC-048, no JSON) records source snapshot SHA, source commit, result SHA and `READ_ONLY_OPERATIONAL`; it grants no qualification or execution authority. The snapshot manifest is `SOURCES.SHA256SUMS`. Existing evidence is never replaced. GUI jobs pin their source snapshot until completion. The UI reports sync time separately from original evidence time.

Commands: `bsc readiness start|stop|status`. Only the existing native `g07_pre_live_readiness --current-rpc --binary` executable is allowed. Its environment contains the public BSC endpoint, no inherited keys or builder credentials. Cancellation forwards SIGTERM to its private process group; never SIGKILL. The Gateway rejects DRY, g07, arm, probe and send. Versioned launch identity is checked again under the controller lock.

Packages: this directory and qualification/status own BSC projection/job behavior; protocol, gateway, frontend and ops own integration under the frontend master plan. No trading guards or historical evidence are edited. Tests use isolated fixtures. Maximum ten BSC test invocations per package.
