use super::*;

fn config() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join(crate::sources::RUNTIME_CONFIG_PATH),
    )
    .unwrap()
}

fn sources(tag: &str) -> PathBuf {
    use std::os::unix::fs::DirBuilderExt;
    let dir = std::env::temp_dir().join(format!("bsc-profiles-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(&dir)
        .unwrap();
    dir
}

/// Every settable value: the configuration file's, `set` over them.
fn values(set: &[(&str, &str)]) -> Vec<(String, String)> {
    settings::effective_values(&config(), &Settings::default())
        .into_iter()
        .map(|(k, v)| {
            let v = set
                .iter()
                .find(|(s, _)| *s == k)
                .map_or(v, |(_, x)| x.to_string());
            (k.to_string(), v)
        })
        .collect()
}

fn create(name: &str, set: &[(&str, &str)]) -> Request {
    Request::Create {
        name: name.into(),
        venues: vec!["BSC_VENUE_FLAP".into()],
        values: values(set),
    }
}

fn update(id: &str, name: &str, set: &[(&str, &str)]) -> Request {
    Request::Update {
        profile_id: id.into(),
        name: name.into(),
        venues: vec!["BSC_VENUE_OPENFOUR".into(), "BSC_VENUE_FLAP".into()],
        values: values(set),
    }
}

fn free(_: &Profile) -> Option<String> {
    None
}

/// BSC-V8-01: create, update, clone an old revision, archive, delete; each
/// a catalog version with its receipt, every revision readable, the same
/// after a reopen (restart).
#[test]
fn a_profile_lives_through_its_revisions_and_a_restart() {
    let dir = sources("crud");
    let text = config();
    let owner = Owner::open(&dir).unwrap();
    let r = owner
        .apply(
            "c1",
            &create("fast", &[("BSC_STRATEGY_C_MAX_OPEN", "3")]),
            0,
            &text,
            &free,
            10,
        )
        .unwrap();
    assert_eq!(
        (
            r.outcome,
            r.catalog_version,
            r.profile_id.as_deref(),
            r.profile_revision
        ),
        ("APPLIED", 1, Some("p000001"), Some(1))
    );
    assert_eq!(
        r.line(),
        "outcome=APPLIED action=CREATE profile_id=p000001 revision=1 catalog=1 request_id=c1"
    );
    let r = owner
        .apply(
            "u1",
            &update("p000001", "faster", &[("BSC_STRATEGY_C_MAX_OPEN", "4")]),
            1,
            &text,
            &free,
            20,
        )
        .unwrap();
    assert_eq!(
        (r.outcome, r.catalog_version, r.profile_revision),
        ("APPLIED", 2, Some(2))
    );
    let p = owner.current().unwrap().profile("p000001").unwrap().clone();
    assert_eq!(
        (
            p.name.as_str(),
            p.revision,
            p.created_unix_ms,
            p.updated_unix_ms,
            p.request_id.as_str()
        ),
        ("faster", 2, 10, 20, "u1")
    );
    // The venues in the owner's order, whatever the request's.
    assert_eq!(p.venues, [VenueId::Flap, VenueId::OpenFour]);
    assert_eq!(p.values.len(), KEYS.len());
    // A clone of revision 1 takes that revision's values.
    let clone = Request::Clone {
        source_profile_id: "p000001".into(),
        source_revision: 1,
        name: "fast again".into(),
    };
    let r = owner.apply("k1", &clone, 2, &text, &free, 30).unwrap();
    assert_eq!(
        (r.outcome, r.profile_id.as_deref()),
        ("APPLIED", Some("p000003"))
    );
    let c = owner.current().unwrap();
    let copy = c.profile("p000003").unwrap();
    assert_eq!(
        copy.values,
        revision(&dir, "p000001", 1).unwrap().unwrap().values
    );
    assert_ne!(
        copy.config_sha256(),
        c.profile("p000001").unwrap().config_sha256()
    );
    assert_eq!(revision(&dir, "p000001", 3).unwrap(), None);
    let archive = Request::Delete {
        profile_id: "p000003".into(),
        archive: true,
    };
    let r = owner.apply("a1", &archive, 1, &text, &free, 40).unwrap();
    assert_eq!((r.outcome, r.profile_revision), ("APPLIED", Some(2)));
    let delete = Request::Delete {
        profile_id: "p000003".into(),
        archive: false,
    };
    assert_eq!(
        owner
            .apply("d1", &delete, 2, &text, &free, 50)
            .unwrap()
            .outcome,
        "APPLIED"
    );
    // Deleted: gone for every request, its values kept.
    let again = Request::Delete {
        profile_id: "p000003".into(),
        archive: false,
    };
    assert_eq!(
        owner
            .apply("d2", &again, 3, &text, &free, 60)
            .unwrap()
            .outcome,
        "REJECTED_NOT_FOUND"
    );
    let (c, file) = current_file(&dir).unwrap();
    assert_eq!((c.version, c.profiles.len()), (5, 2));
    assert_eq!(c.profile("p000003").unwrap().status, Status::Deleted);
    assert!(
        file.unwrap()
            .ends_with("catalog/00000000000000000005.dbrec")
    );
    // Restart: a new owner reads the same, a deleted name is free again.
    drop(owner);
    let owner = Owner::open(&dir).unwrap();
    assert_eq!(owner.current().unwrap(), c);
    let r = owner
        .apply("c2", &create("fast again", &[]), 5, &text, &free, 70)
        .unwrap();
    assert_eq!(r.outcome, "APPLIED");
    std::fs::remove_dir_all(dir).unwrap();
}

/// CAS: one profile edited twice from the same revision, the second is
/// stale; two profiles edited from their revisions both apply; a create
/// on an old catalog version is stale.
#[test]
fn a_stale_revision_is_refused_and_two_profiles_edit_side_by_side() {
    let dir = sources("cas");
    let text = config();
    let owner = Owner::open(&dir).unwrap();
    owner
        .apply("c1", &create("a", &[]), 0, &text, &free, 1)
        .unwrap();
    owner
        .apply("c2", &create("b", &[]), 1, &text, &free, 1)
        .unwrap();
    let r = owner
        .apply(
            "u1",
            &update("p000001", "a", &[("BSC_STRATEGY_C_MAX_OPEN", "2")]),
            1,
            &text,
            &free,
            2,
        )
        .unwrap();
    assert_eq!(r.outcome, "APPLIED");
    let r = owner
        .apply(
            "u2",
            &update("p000001", "a", &[("BSC_STRATEGY_C_MAX_OPEN", "9")]),
            1,
            &text,
            &free,
            2,
        )
        .unwrap();
    assert_eq!(
        (r.outcome, r.profile_revision, r.catalog_version),
        ("REJECTED_STALE", Some(2), 3)
    );
    // The other profile at its own revision 1 applies on catalog 3.
    let r = owner
        .apply(
            "u3",
            &update("p000002", "b", &[("BSC_STRATEGY_C_MAX_OPEN", "7")]),
            1,
            &text,
            &free,
            3,
        )
        .unwrap();
    assert_eq!((r.outcome, r.catalog_version), ("APPLIED", 4));
    let c = owner.current().unwrap();
    let max_open = |id: &str| {
        c.profile(id)
            .unwrap()
            .settings()
            .value("BSC_STRATEGY_C_MAX_OPEN")
            .map(String::from)
    };
    assert_eq!(
        (max_open("p000001"), max_open("p000002")),
        (Some("2".into()), Some("7".into()))
    );
    let r = owner
        .apply("c3", &create("c", &[]), 2, &text, &free, 4)
        .unwrap();
    assert_eq!((r.outcome, r.catalog_version), ("REJECTED_STALE", 4));
    // A name is one profile's.
    let r = owner
        .apply("c4", &create("a", &[]), 4, &text, &free, 4)
        .unwrap();
    assert_eq!(r.outcome, "REJECTED_INVALID");
    assert!(r.reason.unwrap().contains("taken"));
    std::fs::remove_dir_all(dir).unwrap();
}

/// The same request again gets its receipt; its ID with another payload is
/// refused; a value the parser refuses stores nothing; the form is checked
/// before anything (every value, known venues, a name).
#[test]
fn a_request_is_idempotent_and_validated_whole() {
    let dir = sources("idem");
    let text = config();
    let owner = Owner::open(&dir).unwrap();
    let first = owner
        .apply("c1", &create("a", &[]), 0, &text, &free, 1)
        .unwrap();
    assert_eq!(
        owner
            .apply("c1", &create("a", &[]), 0, &text, &free, 9)
            .unwrap(),
        first
    );
    assert!(
        owner
            .apply("c1", &create("b", &[]), 0, &text, &free, 9)
            .is_err()
    );
    assert!(
        owner
            .apply("c1", &create("a", &[]), 1, &text, &free, 9)
            .is_err()
    );
    let r = owner
        .apply(
            "c2",
            &create("b", &[("BSC_STRATEGY_C_MAX_OPEN", "0")]),
            1,
            &text,
            &free,
            2,
        )
        .unwrap();
    assert_eq!(r.outcome, "REJECTED_INVALID");
    assert!(r.reason.unwrap().contains("BSC_STRATEGY_C_MAX_OPEN"));
    assert_eq!(owner.current().unwrap().version, 1);
    let missing = Request::Create {
        name: "c".into(),
        venues: vec!["BSC_VENUE_FLAP".into()],
        values: values(&[]).into_iter().skip(1).collect(),
    };
    assert!(
        owner
            .apply("c3", &missing, 1, &text, &free, 3)
            .unwrap_err()
            .contains("every value")
    );
    let venue = Request::Create {
        name: "c".into(),
        venues: vec!["BSC_VENUE_NONE".into()],
        values: values(&[]),
    };
    assert!(owner.apply("c3", &venue, 1, &text, &free, 3).is_err());
    for name in ["", " c", "c\n"] {
        assert!(
            owner
                .apply("c3", &create(name, &[]), 1, &text, &free, 3)
                .is_err(),
            "{name:?}"
        );
    }
    assert!(
        owner
            .apply("../c3", &create("c", &[]), 1, &text, &free, 3)
            .is_err()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// Crash recovery: a catalog version written, its receipt lost: the same
/// request again is APPLIED at that version, never a second one; for a
/// profile change also after later versions of other profiles.
#[test]
fn a_lost_receipt_is_recovered_from_the_catalog() {
    let dir = sources("crash");
    let text = config();
    let owner = Owner::open(&dir).unwrap();
    let receipts = dir.join(DIR).join("receipts");
    owner
        .apply("c1", &create("a", &[]), 0, &text, &free, 1)
        .unwrap();
    std::fs::remove_file(receipts.join("c1.dbrec")).unwrap();
    let r = owner
        .apply("c1", &create("a", &[]), 0, &text, &free, 2)
        .unwrap();
    assert_eq!(
        (r.outcome, r.catalog_version, r.profile_id.as_deref()),
        ("APPLIED", 1, Some("p000001"))
    );
    owner
        .apply("c2", &create("b", &[]), 1, &text, &free, 3)
        .unwrap();
    let change = update("p000001", "a2", &[]);
    owner.apply("u1", &change, 1, &text, &free, 4).unwrap();
    owner
        .apply("u2", &update("p000002", "b2", &[]), 1, &text, &free, 5)
        .unwrap();
    std::fs::remove_file(receipts.join("u1.dbrec")).unwrap();
    let r = owner.apply("u1", &change, 1, &text, &free, 6).unwrap();
    assert_eq!(
        (r.outcome, r.catalog_version, r.profile_revision),
        ("APPLIED", 3, Some(2))
    );
    assert_eq!(owner.current().unwrap().version, 4);
    std::fs::remove_dir_all(dir).unwrap();
}

/// A profile in use is neither archived nor deleted; the catalog holds at
/// most its configured number of profiles.
#[test]
fn a_used_profile_stays_and_the_catalog_is_bounded() {
    let dir = sources("use");
    let owner = Owner::open(&dir).unwrap();
    let text = settings::overlay(&config(), &[("BSC_STRATEGY_PROFILES_MAX", "2".into())]);
    owner
        .apply("c1", &create("a", &[]), 0, &text, &free, 1)
        .unwrap();
    owner
        .apply("c2", &create("b", &[]), 1, &text, &free, 1)
        .unwrap();
    let r = owner
        .apply("c3", &create("c", &[]), 2, &text, &free, 1)
        .unwrap();
    assert_eq!(r.outcome, "REJECTED_INVALID");
    assert!(r.reason.unwrap().contains("BSC_STRATEGY_PROFILES_MAX"));
    let used = |p: &Profile| (p.profile_id == "p000001").then(|| "training run r1".to_string());
    let delete = Request::Delete {
        profile_id: "p000001".into(),
        archive: false,
    };
    let r = owner.apply("d1", &delete, 1, &text, &used, 2).unwrap();
    assert_eq!(
        (r.outcome, r.reason.as_deref()),
        ("REJECTED_IN_USE", Some("training run r1"))
    );
    assert_eq!(
        owner
            .apply("d2", &delete, 1, &text, &free, 2)
            .unwrap()
            .outcome,
        "APPLIED"
    );
    // A deleted profile frees its place.
    assert_eq!(
        owner
            .apply("c4", &create("c", &[]), 3, &text, &free, 3)
            .unwrap()
            .outcome,
        "APPLIED"
    );
    let clone = Request::Clone {
        source_profile_id: "p000002".into(),
        source_revision: 5,
        name: "d".into(),
    };
    let r = owner.apply("k1", &clone, 4, &text, &free, 4).unwrap();
    assert_eq!(
        r.outcome, "REJECTED_INVALID",
        "full before the source is looked up"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// Review M1: a request ID is one request's even when its receipt is lost:
/// a retry under another CAS or payload is refused (never a second
/// version), the same one recovered; a clone, an archive and a delete too.
#[test]
fn a_request_id_is_one_request_even_without_its_receipt() {
    let dir = sources("intent");
    let text = config();
    let owner = Owner::open(&dir).unwrap();
    let receipts = dir.join(DIR).join("receipts");
    let lose = |id: &str| std::fs::remove_file(receipts.join(format!("{id}.dbrec"))).unwrap();
    owner
        .apply("c1", &create("a", &[]), 0, &text, &free, 1)
        .unwrap();
    owner
        .apply("u1", &update("p000001", "a1", &[]), 1, &text, &free, 2)
        .unwrap();
    lose("u1");
    // The reviewer's repros: another CAS under the lost receipt's ID.
    assert!(
        owner
            .apply("u1", &update("p000001", "a1", &[]), 2, &text, &free, 3)
            .is_err()
    );
    lose("c1");
    assert!(
        owner
            .apply("c1", &create("a", &[]), 2, &text, &free, 3)
            .is_err()
    );
    assert!(
        owner
            .apply("c1", &create("b", &[]), 0, &text, &free, 3)
            .is_err()
    );
    assert_eq!(owner.current().unwrap().version, 2);
    // The same requests again: their versions.
    let r = owner
        .apply("c1", &create("a", &[]), 0, &text, &free, 4)
        .unwrap();
    assert_eq!(
        (r.outcome, r.catalog_version, r.profile_id.as_deref()),
        ("APPLIED", 1, Some("p000001"))
    );
    let clone = Request::Clone {
        source_profile_id: "p000001".into(),
        source_revision: 1,
        name: "b".into(),
    };
    owner.apply("k1", &clone, 2, &text, &free, 5).unwrap();
    let archive = Request::Delete {
        profile_id: "p000003".into(),
        archive: true,
    };
    owner.apply("a1", &archive, 1, &text, &free, 6).unwrap();
    let delete = Request::Delete {
        profile_id: "p000003".into(),
        archive: false,
    };
    owner.apply("d1", &delete, 2, &text, &free, 7).unwrap();
    owner
        .apply("u2", &update("p000001", "a2", &[]), 2, &text, &free, 8)
        .unwrap();
    for (id, request, expected, version) in [
        ("k1", &clone, 2, 3),
        ("a1", &archive, 1, 4),
        ("d1", &delete, 2, 5),
    ] {
        lose(id);
        let r = owner.apply(id, request, expected, &text, &free, 9).unwrap();
        assert_eq!((r.outcome, r.catalog_version), ("APPLIED", version), "{id}");
    }
    assert_eq!(owner.current().unwrap().version, 6);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Review M2: the command line takes each flag's value from its own pair:
/// a name like `--request-id` is a name; every flag once, none missing,
/// value pairs only for a create or an update.
#[test]
fn the_command_line_takes_each_flag_from_its_pair() {
    let line = |w: &[&str]| w.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let tail = [
        "--expect",
        "3",
        "--request-id",
        "c2",
        "--state",
        "/s",
        "--config",
        "/c",
    ];
    let mut args = line(&[
        "BSC_STRATEGY_C_MAX_OPEN=3",
        "--name",
        "--request-id",
        "--venues",
        "BSC_VENUE_FLAP",
    ]);
    args.extend(line(&tail));
    let c = command("create", &args).unwrap();
    assert_eq!(
        (c.request_id.as_str(), c.state.as_str(), c.expected),
        ("c2", "/s", 3)
    );
    assert_eq!(
        c.request,
        Request::Create {
            name: "--request-id".into(),
            venues: vec!["BSC_VENUE_FLAP".into()],
            values: vec![("BSC_STRATEGY_C_MAX_OPEN".into(), "3".into())],
        }
    );
    let mut twice = args.clone();
    twice.extend(line(&["--expect", "4"]));
    let mut missing = line(&["--name", "n", "--venues", "BSC_VENUE_FLAP"]);
    missing.extend(line(&tail[2..]));
    let mut valued_clone = line(&[
        "BSC_STRATEGY_C_MAX_OPEN=3",
        "--source",
        "p1",
        "--source-revision",
        "1",
        "--name",
        "n",
    ]);
    valued_clone.extend(line(&tail));
    let mut odd = args.clone();
    odd.push("--name".into());
    for (cmd, a) in [
        ("create", twice),
        ("create", missing),
        ("clone", valued_clone),
        ("create", odd),
        ("rename", args.clone()),
    ] {
        assert!(command(cmd, &a).is_err(), "{cmd} {a:?}");
    }
    let mut delete = line(&["--profile", "p000001"]);
    delete.extend(line(&tail));
    assert_eq!(
        command("archive", &delete).unwrap().request,
        Request::Delete {
            profile_id: "p000001".into(),
            archive: true
        }
    );
}

/// Review L1/L2/L3/L6: the largest CAS is stale, never a panic; an
/// unreadable old version is an error without a receipt (retry); a profile
/// in use is not changed; an ID out of shape is refused.
#[test]
fn edges_are_refusals_or_errors_never_panics() {
    let dir = sources("edges");
    let text = config();
    let owner = Owner::open(&dir).unwrap();
    owner
        .apply("c1", &create("a", &[]), 0, &text, &free, 1)
        .unwrap();
    owner
        .apply(
            "u1",
            &update("p000001", "a", &[("BSC_STRATEGY_C_MAX_OPEN", "2")]),
            1,
            &text,
            &free,
            2,
        )
        .unwrap();
    let r = owner
        .apply(
            "u2",
            &update("p000001", "a", &[]),
            u64::MAX,
            &text,
            &free,
            3,
        )
        .unwrap();
    assert_eq!(r.outcome, "REJECTED_STALE");
    let r = owner
        .apply("c2", &create("b", &[]), u64::MAX, &text, &free, 3)
        .unwrap();
    assert_eq!(r.outcome, "REJECTED_STALE");
    let used = |_: &Profile| Some("the LIVE selection".to_string());
    let r = owner
        .apply("u3", &update("p000001", "a", &[]), 2, &text, &used, 4)
        .unwrap();
    assert_eq!(
        (r.outcome, r.reason.as_deref()),
        ("REJECTED_IN_USE", Some("the LIVE selection"))
    );
    for bad in ["p\n1", "../p1", &"p".repeat(40)] {
        assert!(
            owner
                .apply("u4", &update(bad, "a", &[]), 2, &text, &free, 4)
                .is_err(),
            "{bad:?}"
        );
    }
    // Revision 1 of p000001 lives only in version 1: unreadable, no receipt.
    std::fs::write(
        dir.join(DIR).join("catalog/00000000000000000001.dbrec"),
        b"broken",
    )
    .unwrap();
    let clone = Request::Clone {
        source_profile_id: "p000001".into(),
        source_revision: 1,
        name: "c".into(),
    };
    assert!(owner.apply("k1", &clone, 2, &text, &free, 5).is_err());
    assert!(!dir.join(DIR).join("receipts/k1.dbrec").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

/// Review L4/L5: a deleted profile lives on only in the version its
/// deletion wrote (later versions leave it out, its revisions stay
/// readable); every revision of a profile reads back across many versions;
/// the configured maximum is bounded by what one version holds.
#[test]
fn deleted_profiles_leave_the_catalog_and_revisions_read_back() {
    let dir = sources("prune");
    let text = config();
    let owner = Owner::open(&dir).unwrap();
    owner
        .apply("c1", &create("a", &[]), 0, &text, &free, 1)
        .unwrap();
    let mut version = 1;
    // p000001 at revisions 1..=6, other profiles created between.
    for r in 1..=5u64 {
        let max_open = (r + 1).to_string();
        owner
            .apply(
                &format!("u{r}"),
                &update("p000001", "a", &[("BSC_STRATEGY_C_MAX_OPEN", &max_open)]),
                r,
                &text,
                &free,
                2,
            )
            .unwrap();
        version += 1;
        owner
            .apply(
                &format!("o{r}"),
                &create(&format!("o{r}"), &[]),
                version,
                &text,
                &free,
                2,
            )
            .unwrap();
        version += 1;
    }
    for r in 1..=6u64 {
        let p = revision(&dir, "p000001", r).unwrap().unwrap();
        assert_eq!(p.revision, r);
        assert_eq!(
            p.settings().value("BSC_STRATEGY_C_MAX_OPEN"),
            Some(&*if r == 1 {
                "5".to_string()
            } else {
                r.to_string()
            })
        );
    }
    assert_eq!(revision(&dir, "p000001", 0).unwrap(), None);
    assert_eq!(revision(&dir, "p000001", 7).unwrap(), None);
    let delete = Request::Delete {
        profile_id: "p000003".into(),
        archive: false,
    };
    let r = owner.apply("d1", &delete, 1, &text, &free, 3).unwrap();
    assert_eq!(r.outcome, "APPLIED");
    let deleted = owner.current().unwrap();
    assert_eq!(deleted.profile("p000003").unwrap().status, Status::Deleted);
    owner
        .apply("c9", &create("z", &[]), deleted.version, &text, &free, 4)
        .unwrap();
    let after = owner.current().unwrap();
    assert!(after.profile("p000003").is_none());
    assert_eq!(after.profiles.len(), deleted.profiles.len());
    assert_eq!(
        revision(&dir, "p000003", 2).unwrap().unwrap().status,
        Status::Deleted
    );
    let bound = max_profiles_bound();
    assert!(bound >= 64, "{bound}");
    let over = settings::overlay(
        &config(),
        &[("BSC_STRATEGY_PROFILES_MAX", (bound + 1).to_string())],
    );
    assert!(RuntimeConfig::parse(&over).is_err());
    let at = settings::overlay(
        &config(),
        &[("BSC_STRATEGY_PROFILES_MAX", bound.to_string())],
    );
    assert!(RuntimeConfig::parse(&at).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}

/// Review L8: profiles edited at once from one catalog version (each from
/// its own revision) all apply, each once; the losers of a version decide
/// again on the winner's catalog.
#[test]
fn concurrent_edits_of_different_profiles_all_apply() {
    let dir = sources("threads");
    let text = config();
    let owner = Owner::open(&dir).unwrap();
    let n = 6u64;
    for i in 0..n {
        owner
            .apply(
                &format!("c{i}"),
                &create(&format!("p{i}"), &[]),
                i,
                &text,
                &free,
                1,
            )
            .unwrap();
    }
    std::thread::scope(|s| {
        for i in 0..n {
            let (dir, text) = (&dir, &text);
            s.spawn(move || {
                let owner = Owner::open(dir).unwrap();
                let id = format!("p{:06}", i + 1);
                let r = owner
                    .apply(
                        &format!("u{i}"),
                        &update(&id, &format!("q{i}"), &[]),
                        1,
                        text,
                        &free,
                        2,
                    )
                    .unwrap();
                assert_eq!(
                    (r.outcome, r.profile_revision),
                    ("APPLIED", Some(2)),
                    "{id}"
                );
            });
        }
    });
    let c = owner.current().unwrap();
    assert_eq!(c.version, 2 * n);
    assert!(
        c.profiles
            .iter()
            .all(|p| p.revision == 2 && p.name.starts_with('q'))
    );
    std::fs::remove_dir_all(dir).unwrap();
}
