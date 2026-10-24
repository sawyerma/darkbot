// Conservative tone of a native value: only a qualified pass is green. G0.7 classifications
// are never a success (G0.8 decides); unknown or unclassified values stay visibly neutral.
export function statusTone(state:string): "green"|"red"|"amber"|"neutral" {
 if (["PASS","PASS_READ_ONLY","RUNNING"].includes(state)) return "green";
 if (["FAIL","FAILED","STOP","BLOCKED","ARMED_MISMATCH"].includes(state) || state.startsWith("STOP_")) return "red";
 if (["ARMED","UNARMED","STOPPING","PARTIAL","UNQUALIFIED","ADAPT","UNRESOLVED","STALE"].includes(state)
  || state.startsWith("PROVISIONAL_RELEASED__") || state.startsWith("SUBMITTED_OR_SUBMISSION_UNKNOWN__")) return "amber";
 return "neutral";
}
