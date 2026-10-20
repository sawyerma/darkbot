export function statusTone(state:string): "green"|"red"|"amber"|"neutral" {
 if (["PASS","PASS_READ_ONLY","RUNNING"].includes(state)) return "green";
 if (["FAIL","FAILED","STOP","BLOCKED","ARMED_MISMATCH"].includes(state)) return "red";
 if (["ARMED","UNARMED","STOPPING","PARTIAL","UNQUALIFIED"].includes(state)) return "amber";
 return "neutral";
}
