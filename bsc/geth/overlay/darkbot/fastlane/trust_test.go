package fastlane

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func trustVictimV2(t *testing.T) string {
	t.Helper()
	victim := filepath.Join(t.TempDir(), "victim")
	if err := os.WriteFile(victim, []byte("keep"), 0o600); err != nil {
		t.Fatal(err)
	}
	return victim
}

func trustVictimIntactV2(t *testing.T, victim string) {
	t.Helper()
	data, err := os.ReadFile(victim)
	if err != nil || string(data) != "keep" {
		t.Fatalf("victim changed: %q %v", data, err)
	}
}

func trustLaneV2() []laneConfigV2 {
	return []laneConfigV2{{ID: 1, Direction: v2GethToRust, Class: v2TxpoolEvidence, Capacity: 4, SlotStride: 512}}
}

func TestE75LockSymlinkAndHardlinkAreNeverTruncated(t *testing.T) {
	for _, link := range []func(string, string) error{os.Symlink, os.Link} {
		dir := t.TempDir()
		victim := trustVictimV2(t)
		if err := link(victim, filepath.Join(dir, v2RoleGethOwner+".lock")); err != nil {
			t.Fatal(err)
		}
		if lease, err := acquireRoleLeaseV2(dir, v2RoleGethOwner, 1); err == nil {
			_ = lease.release()
			t.Fatal("linked lock file was accepted")
		}
		trustVictimIntactV2(t, victim)
	}
}

func TestE75UntrustedDirectoriesFailClosed(t *testing.T) {
	for _, mode := range []os.FileMode{0o770, 0o702, 0o775, 0o777} {
		dir := t.TempDir()
		if err := os.Chmod(dir, mode); err != nil {
			t.Fatal(err)
		}
		if _, err := acquireRoleLeaseV2(dir, v2RoleGethOwner, 1); !errors.Is(err, errV2UntrustedPath) {
			t.Fatalf("mode %o lease: %v", mode, err)
		}
		if _, err := createArenaV2(arenaPathV2(dir, 1), 1, trustLaneV2()); !errors.Is(err, errV2UntrustedPath) {
			t.Fatalf("mode %o arena: %v", mode, err)
		}
		if _, err := staleArenasV2(dir, 1); !errors.Is(err, errV2UntrustedPath) {
			t.Fatalf("mode %o stale listing: %v", mode, err)
		}
		cfg := c14TestConfig(t, 1, 0, 8, 512)
		cfg.Dir = dir
		if _, err := startIngressRuntimeV2(cfg); !errors.Is(err, errV2UntrustedPath) {
			t.Fatalf("mode %o ingress runtime: %v", mode, err)
		}
	}
}

func TestE75AncestorsNeedOwnerAndStickyWhenShared(t *testing.T) {
	parent := t.TempDir()
	dir := filepath.Join(parent, "fastlane")
	if err := os.Mkdir(dir, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(parent, 0o777); err != nil {
		t.Fatal(err)
	}
	if err := checkTrustedFastLaneDirV2(dir); !errors.Is(err, errV2UntrustedPath) {
		t.Fatalf("world-writable ancestor without sticky bit: %v", err)
	}
	if err := os.Chmod(parent, 0o777|os.ModeSticky); err != nil {
		t.Fatal(err)
	}
	if err := checkTrustedFastLaneDirV2(dir); err != nil {
		t.Fatalf("sticky shared ancestor: %v", err)
	}
	// A symlinked component resolves to the trusted directory; the walk
	// checks the resolved path.
	alias := filepath.Join(t.TempDir(), "alias")
	if err := os.Symlink(dir, alias); err != nil {
		t.Fatal(err)
	}
	if err := checkTrustedFastLaneDirV2(alias); err != nil {
		t.Fatalf("alias of a trusted directory: %v", err)
	}
	open := filepath.Join(t.TempDir(), "open")
	if err := os.Mkdir(open, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.Chmod(open, 0o777); err != nil {
		t.Fatal(err)
	}
	openAlias := filepath.Join(t.TempDir(), "alias")
	if err := os.Symlink(open, openAlias); err != nil {
		t.Fatal(err)
	}
	if err := checkTrustedFastLaneDirV2(openAlias); !errors.Is(err, errV2UntrustedPath) {
		t.Fatalf("alias of an open directory: %v", err)
	}
}

func TestE75ArenaOpensRefuseSymlinksAndOpenModes(t *testing.T) {
	dir := t.TempDir()
	other := t.TempDir()
	real := arenaPathV2(other, 1)
	arena, err := createArenaV2(real, 1, trustLaneV2())
	if err != nil {
		t.Fatal(err)
	}
	defer arena.close()
	path := arenaPathV2(dir, 1)
	if err := os.Symlink(real, path); err != nil {
		t.Fatal(err)
	}
	if a, err := openArenaV2(path, 1); err == nil {
		_ = a.close()
		t.Fatal("symlinked arena was mapped")
	}
	if _, _, err := probeArenaV2(path, 1); err == nil {
		t.Fatal("symlinked arena was probed")
	}
	victim := trustVictimV2(t)
	victimLink := arenaPathV2(dir, 2)
	if err := os.Symlink(victim, victimLink); err != nil {
		t.Fatal(err)
	}
	if a, err := createArenaV2(victimLink, 2, trustLaneV2()); err == nil {
		_ = a.close()
		t.Fatal("arena created through a symlink")
	}
	trustVictimIntactV2(t, victim)
	if err := os.Chmod(real, 0o644); err != nil {
		t.Fatal(err)
	}
	if a, err := openArenaV2(real, 1); !errors.Is(err, errV2UntrustedPath) {
		if a != nil {
			_ = a.close()
		}
		t.Fatalf("arena with group/other bits: %v", err)
	}
	if err := os.Chmod(real, 0o600); err != nil {
		t.Fatal(err)
	}
	reopened, err := openArenaV2(real, 1)
	if err != nil {
		t.Fatalf("trusted arena: %v", err)
	}
	_ = reopened.close()
	if probe, _, err := probeArenaV2(arenaPathV2(other, 9), 9); err != nil || probe != arenaAbsentV2 {
		t.Fatalf("absent arena probe: %v %v", probe, err)
	}
}
