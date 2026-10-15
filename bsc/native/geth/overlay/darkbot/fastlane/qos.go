// C1.6: macOS QoS for permanent latency-critical goroutines.
//
// A QoS class is a property of an OS thread. Only a goroutine that lives for
// the process and is bound to its own thread can carry one, so only the C1.7
// outbound coordinator and workers request it. Peer-ingress handlers are
// ordinary goroutines that the Go scheduler moves between threads; no QoS
// class is requested for them.
//
// QoS is advisory. It is not CPU pinning and no performance core is
// promised. It is off unless DARKBOT_FASTLANE_QOS names a class; when it is
// off, or the host has no thread QoS, scheduling is unchanged and the
// evidence says so.

package fastlane

import (
	"os"
	"runtime"
	"strings"
	"sync"
)

const envPermanentThreadQoS = "DARKBOT_FASTLANE_QOS"

// qosClassesV2 maps the configured names to the qos_class_t values of
// <sys/qos.h>.
var qosClassesV2 = map[string]uint32{
	"user-interactive": 0x21,
	"user-initiated":   0x19,
	"default":          0x15,
	"utility":          0x11,
	"background":       0x09,
}

var permanentThreadQoS struct {
	sync.Mutex
	outcome map[string]string
}

// applyPermanentThreadQoS is called once by a permanent goroutine when it
// starts. With a configured class it binds the goroutine to its OS thread and
// requests the class; the outcome is recorded per role.
func applyPermanentThreadQoS(role string) {
	name := strings.TrimSpace(os.Getenv(envPermanentThreadQoS))
	outcome := "not-requested"
	if name != "" && name != "off" {
		class, known := qosClassesV2[name]
		switch {
		case !known:
			outcome = "refused:unknown-class:" + name
		case !threadQoSSupported:
			outcome = "unsupported:" + threadQoSUnsupportedReason
		default:
			runtime.LockOSThread()
			if err := setThreadQoS(class); err != nil {
				outcome = "failed:" + name + ":" + err.Error()
			} else {
				outcome = "applied:" + name
			}
		}
	}
	permanentThreadQoS.Lock()
	if permanentThreadQoS.outcome == nil {
		permanentThreadQoS.outcome = make(map[string]string)
	}
	permanentThreadQoS.outcome[role] = outcome
	permanentThreadQoS.Unlock()
}

// permanentThreadQoSSnapshot returns the recorded outcome per role.
func permanentThreadQoSSnapshot() map[string]string {
	permanentThreadQoS.Lock()
	defer permanentThreadQoS.Unlock()
	out := make(map[string]string, len(permanentThreadQoS.outcome))
	for role, outcome := range permanentThreadQoS.outcome {
		out[role] = outcome
	}
	return out
}
