//go:build !darwin || !cgo

package fastlane

import "errors"

const (
	threadQoSSupported         = false
	threadQoSUnsupportedReason = "thread QoS classes need macOS and cgo"
)

// setThreadQoS reports that this build has no thread QoS.
func setThreadQoS(uint32) error {
	return errors.New(threadQoSUnsupportedReason)
}
