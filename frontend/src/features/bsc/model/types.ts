// Read-only domain projection. Amounts remain exact decimal wei; absent means unknown.
export interface PublicField { name:string; value:string }
export interface Evidence { path:string; sha256:string; observedUnixS:bigint }
export interface Guard { guardState:string; frozenApprovedSha256:string; contractPresent:boolean; contractPath:string; contractSha256:string; approved?:boolean; executionReady?:boolean; attemptId:string; maxAttempts?:bigint }
export interface BscStatusView {
 sourceCommit:string; evaluatedUnixS:bigint; dataAsOfUnixS:bigint; liveState:string;
 guard?:Guard; gates:{gateId:string;state:string;evidence?:Evidence;reasonCodes:string[]}[];
 readiness?:{verdict:string; evidence?:Evidence; fundingVerdict:string; checks:PublicField[]; portalVersionBound:string;portalVersionObserved:string;portalImplBound:string;portalImplObserved:string;codeHashChanged?:boolean;reasons:string[]};
 builder?:{refreshObservedUnixS:bigint;refreshExpiresUnixS:bigint;floors:PublicField[];probe?:Evidence;price:string;noMerge:string;positionFirst:string;inclusion:string};
 witness?:{sourceReady?:boolean;t0ToBuyState:string;evidence?:Evidence};
 dry?:{evidence?:Evidence;dryId:string;state:string;parameters:PublicField[];validForCurrentPortal?:boolean};
 jobs:{job:string;state:string;launchId:string;resultCode:string}[]; warnings:string[];
}
export interface BscLaunchView {contractPresent:boolean;evidence?:Evidence;contractFields:PublicField[];runtimeFields:PublicField[];reason:string}
export interface BscMonitoringView {state:string;evidence?:Evidence;results:PublicField[]}
export interface BscWalletsView {expectedAddress:string;walletStatus:string;launcherAddress:string;buyerAddress:string;chainId?:bigint;launcherBalanceWei?:string;buyerBalanceWei?:string;observedBlock?:bigint;observedUnixS:bigint;source:string;requiredLauncherWei?:string;requiredBuyerWei?:string;evidence?:Evidence}
