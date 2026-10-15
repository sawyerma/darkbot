//go:build cgo

package fastlane

/*
#include <pthread.h>
#include <pthread/qos.h>

static int darkbot_set_thread_qos(unsigned int qos_class) {
	return pthread_set_qos_class_self_np((qos_class_t)qos_class, 0);
}
*/
import "C"

import "fmt"

const (
	threadQoSSupported         = true
	threadQoSUnsupportedReason = ""
)

// setThreadQoS requests a QoS class for the calling OS thread.
func setThreadQoS(class uint32) error {
	if rc := C.darkbot_set_thread_qos(C.uint(class)); rc != 0 {
		return fmt.Errorf("pthread_set_qos_class_self_np errno %d", int(rc))
	}
	return nil
}
