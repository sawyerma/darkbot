#!/usr/bin/env bash
set -euo pipefail

PIN_COMMIT="cdb7548b5baacfdae92f9f63437d6456411665f3"
PIN_TREE="d8d88097521d43cef66047fdd93dd6825a2a7843"
PIN_TAG="v1.7.8"
PIN_MODULE="github.com/ethereum/go-ethereum"
PIN_GO="1.25.0"

SOURCE="${1:-${BSC_GETH_SOURCE:-}}"
if [[ -z "${SOURCE}" ]]; then
  printf 'usage: %s /path/to/bsc-geth\n' "$0" >&2
  exit 2
fi

if [[ ! -d "${SOURCE}/.git" ]]; then
  printf 'not a git checkout: %s\n' "${SOURCE}" >&2
  exit 2
fi

HEAD="$(git -C "${SOURCE}" rev-parse HEAD)"
TREE="$(git -C "${SOURCE}" rev-parse 'HEAD^{tree}')"
TAG="$(git -C "${SOURCE}" tag --points-at HEAD | grep -Fx "${PIN_TAG}" || true)"
MODULE="$(awk '$1 == "module" { print $2; exit }' "${SOURCE}/go.mod")"
GO_VERSION="$(awk '$1 == "go" { print $2; exit }' "${SOURCE}/go.mod")"

[[ "${HEAD}" == "${PIN_COMMIT}" ]] || {
  printf 'C1_GETH_PIN_FAIL head=%s expected=%s\n' "${HEAD}" "${PIN_COMMIT}" >&2
  exit 1
}
[[ "${TREE}" == "${PIN_TREE}" ]] || {
  printf 'C1_GETH_PIN_FAIL tree=%s expected=%s\n' "${TREE}" "${PIN_TREE}" >&2
  exit 1
}
[[ "${TAG}" == "${PIN_TAG}" ]] || {
  printf 'C1_GETH_PIN_FAIL tag=%s expected=%s\n' "${TAG:-NONE}" "${PIN_TAG}" >&2
  exit 1
}
[[ "${MODULE}" == "${PIN_MODULE}" ]] || {
  printf 'C1_GETH_PIN_FAIL module=%s expected=%s\n' "${MODULE}" "${PIN_MODULE}" >&2
  exit 1
}
[[ "${GO_VERSION}" == "${PIN_GO}" ]] || {
  printf 'C1_GETH_PIN_FAIL go=%s expected=%s\n' "${GO_VERSION}" "${PIN_GO}" >&2
  exit 1
}

if ! git -C "${SOURCE}" diff --quiet || ! git -C "${SOURCE}" diff --cached --quiet; then
  printf 'C1_GETH_PIN_FAIL tracked tree is dirty\n' >&2
  exit 1
fi

printf 'C1_GETH_PIN_PASS\n'
printf 'commit=%s\n' "${HEAD}"
printf 'tree=%s\n' "${TREE}"
printf 'tag=%s\n' "${TAG}"
printf 'module=%s\n' "${MODULE}"
printf 'go=%s\n' "${GO_VERSION}"
