"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __exportStar = (this && this.__exportStar) || function(m, exports) {
    for (var p in m) if (p !== "default" && !Object.prototype.hasOwnProperty.call(exports, p)) __createBinding(exports, m, p);
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.cliqueVerifySignature = exports.cliqueSigner = exports.cliqueSigHash = exports.cliqueIsEpochTransition = exports.cliqueExtraVanity = exports.cliqueExtraSeal = exports.cliqueEpochTransitionSigners = exports.CLIQUE_EXTRA_VANITY = exports.CLIQUE_EXTRA_SEAL = void 0;
var clique_ts_1 = require("./clique.js");
Object.defineProperty(exports, "CLIQUE_EXTRA_SEAL", { enumerable: true, get: function () { return clique_ts_1.CLIQUE_EXTRA_SEAL; } });
Object.defineProperty(exports, "CLIQUE_EXTRA_VANITY", { enumerable: true, get: function () { return clique_ts_1.CLIQUE_EXTRA_VANITY; } });
Object.defineProperty(exports, "cliqueEpochTransitionSigners", { enumerable: true, get: function () { return clique_ts_1.cliqueEpochTransitionSigners; } });
Object.defineProperty(exports, "cliqueExtraSeal", { enumerable: true, get: function () { return clique_ts_1.cliqueExtraSeal; } });
Object.defineProperty(exports, "cliqueExtraVanity", { enumerable: true, get: function () { return clique_ts_1.cliqueExtraVanity; } });
Object.defineProperty(exports, "cliqueIsEpochTransition", { enumerable: true, get: function () { return clique_ts_1.cliqueIsEpochTransition; } });
Object.defineProperty(exports, "cliqueSigHash", { enumerable: true, get: function () { return clique_ts_1.cliqueSigHash; } });
Object.defineProperty(exports, "cliqueSigner", { enumerable: true, get: function () { return clique_ts_1.cliqueSigner; } });
Object.defineProperty(exports, "cliqueVerifySignature", { enumerable: true, get: function () { return clique_ts_1.cliqueVerifySignature; } });
__exportStar(require("./ethash.js"), exports);
//# sourceMappingURL=index.js.map