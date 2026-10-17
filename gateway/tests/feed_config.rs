use darkbot_gateway::config::parse_feeds;
use darkbot_gateway::proto::ProjectId;

#[test]
fn one_generic_feed_list_and_reject_duplicates() {
    let feeds = parse_feeds("project0=/tmp/project0.sock,bsc=/tmp/bsc.sock").unwrap();
    assert_eq!(feeds.len(), 2);
    assert_eq!(feeds[0].project, ProjectId::Project0);
    assert_eq!(feeds[1].project, ProjectId::Bsc);
    for bad in [
        "project0=relative.sock",
        "project0=/tmp/a,project0=/tmp/b",
        "other=/tmp/test.sock",
        "project0=",
    ] {
        assert!(parse_feeds(bad).is_err(), "accepted {bad}");
    }
}
