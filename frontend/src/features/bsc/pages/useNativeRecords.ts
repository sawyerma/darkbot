import { useMemo } from "react";
import { selectLaunchRecords, selectMonitoringRecords, useBscStore } from "../store/bscStore";
import type { NativeRecordView } from "../model/types";

/** Native records of the launch and monitoring topics; each leaf is placed by the manifest. */
export function useNativeRecords(): readonly NativeRecordView[] {
  const launch = useBscStore(selectLaunchRecords);
  const monitoring = useBscStore(selectMonitoringRecords);
  return useMemo(() => [...launch, ...monitoring], [launch, monitoring]);
}
