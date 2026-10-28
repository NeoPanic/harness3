use harness3_platform::{
    channels::{Actor, ChannelMembers, ChannelPost, ChannelSubscription, CreateChannel},
    domain::*,
    store::Store,
};
use serde_json::{Value, json};
fn setup() -> (Store, String, Vec<String>) {
    setup_with_plugins(json!([{"id":"platform.channels","version":"1.0.0"}]))
}
fn setup_with_plugins(plugins: Value) -> (Store, String, Vec<String>) {
    let s = Store::memory().unwrap();
    s.publish_definition(json!({"name":"channel-team","version":"1","purpose":"team discussion","objective":"share evidence","instructions":"publish useful findings","protocol":"demo","plugins":plugins})).unwrap();
    let f = s
        .spawn(
            &Spawn {
                preset_name: "channel-team".into(),
                preset_version: "1".into(),
                count: 3,
            },
            "agents",
        )
        .unwrap();
    let agents = f["instance_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().into())
        .collect::<Vec<String>>();
    let g = s
        .create_goal(
            &GoalPlan {
                verification: None,
                title: "channel goal".into(),
                objective: "share discoveries".into(),
                acceptance: "evidence traceable".into(),
                tasks: agents
                    .iter()
                    .enumerate()
                    .map(|(i, id)| TaskPlan {
                        key: format!("task-{i}"),
                        instance_id: id.clone(),
                        instructions: "inspect evidence".into(),
                        ..Default::default()
                    })
                    .collect(),
            },
            "goal",
        )
        .unwrap();
    (s, g["id"].as_str().unwrap().into(), agents)
}
fn create(s: &Store, g: &str, members: &[String], name: &str) -> String {
    s.create_channel(
        &CreateChannel {
            goal_id: g.into(),
            name: name.into(),
            description: "shared findings".into(),
            members: members.to_vec(),
        },
        name,
    )
    .unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}
fn finish(s: &Store, r: &Value) {
    s.finish(
        r["id"].as_str().unwrap(),
        r["instance_id"].as_str().unwrap(),
        "completed",
        json!({"output":"evidence"}),
    )
    .unwrap();
}
#[test]
fn group_visibility_identity_and_durable_posts_are_host_scoped() {
    let (s, g, a) = setup();
    let c = create(&s, &g, &a[..2], "group-a");
    let d = create(&s, &g, &a[2..], "group-b");
    s.reconcile_goals().unwrap();
    let r = (0..3)
        .map(|_| s.claim().unwrap().unwrap())
        .collect::<Vec<_>>();
    let source = r.iter().find(|r| r["instance_id"] == a[0]).unwrap();
    let run = source["id"].as_str().unwrap();
    let visible = s.run_channels(run).unwrap();
    let names = visible["channels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(names.contains(&"group-a"));
    assert!(!names.contains(&"group-b"));
    assert_eq!(
        s.channel(Actor::AgentRun(run), &d).unwrap_err().1,
        "channel_out_of_scope"
    );
    let p = ChannelPost {
        text: "evidence from group A".into(),
        wake_subscribers: true,
    };
    let receipt = s
        .post_channel(Actor::AgentRun(run), &c, &p, "publish")
        .unwrap();
    assert_eq!(
        s.post_channel(Actor::AgentRun(run), &c, &p, "publish")
            .unwrap(),
        receipt
    );
    let view = s.channel(Actor::Operator, &c).unwrap();
    assert_eq!(view["posts"][0]["source_run_id"], run);
    assert_eq!(view["posts"][0]["instance_id"], a[0]);
    assert_eq!(view["posts"].as_array().unwrap().len(), 1);
    assert_eq!(
        s.subscribe_channel(
            Actor::AgentRun(run),
            &c,
            &ChannelSubscription {
                instance_id: Some(a[1].clone()),
                subscribed: true
            },
            "impersonate"
        )
        .unwrap_err()
        .1,
        "channel_subscription_identity_is_host_owned"
    );
}
#[test]
fn subscriptions_wake_once_and_notification_replies_do_not_broadcast() {
    let (s, g, a) = setup();
    let c = create(&s, &g, &a, "team");
    for id in &a {
        s.subscribe_channel(
            Actor::Operator,
            &c,
            &ChannelSubscription {
                instance_id: Some(id.clone()),
                subscribed: true,
            },
            id,
        )
        .unwrap();
    }
    s.reconcile_goals().unwrap();
    let r = (0..3)
        .map(|_| s.claim().unwrap().unwrap())
        .collect::<Vec<_>>();
    let source = r.iter().find(|r| r["instance_id"] == a[0]).unwrap();
    let receipt = s
        .post_channel(
            Actor::AgentRun(source["id"].as_str().unwrap()),
            &c,
            &ChannelPost {
                text: "new useful finding".into(),
                wake_subscribers: true,
            },
            "finding",
        )
        .unwrap();
    assert_eq!(receipt["notification_count"], 2);
    for r in &r {
        finish(&s, r);
    }
    s.reconcile_channels().unwrap();
    s.reconcile_channels().unwrap();
    let pending = s.recent_messages().unwrap();
    assert_eq!(pending.len(), 2);
    s.reconcile_messages().unwrap();
    let wake = s.claim().unwrap().unwrap();
    assert!(
        wake["input"]
            .as_str()
            .unwrap()
            .contains("Channel notification")
    );
    let reply = s
        .post_channel(
            Actor::AgentRun(wake["id"].as_str().unwrap()),
            &c,
            &ChannelPost {
                text: "related evidence after notification".into(),
                wake_subscribers: true,
            },
            "reply",
        )
        .unwrap();
    assert_eq!(reply["notification_reply_suppressed"], true);
    assert_eq!(reply["notification_count"], 0);
    s.reconcile_channels().unwrap();
    assert_eq!(s.recent_messages().unwrap().len(), 2);
}
#[test]
fn notification_run_reads_peer_files_updates_shared_context_and_replies_without_rebroadcast() {
    use harness3_platform::{
        autonomy::NextInput,
        collaboration::{Author, ContextInput},
        workspaces::{PublishInput, WriteInput},
    };
    struct Workspace(std::path::PathBuf);
    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = Workspace(
        std::env::temp_dir().join(format!("h3-channel-workspace-{}", uuid::Uuid::new_v4())),
    );
    let (s, g, a) = setup_with_plugins(json!([
        {"id":"platform.channels","version":"1.0.0"},
        {"id":"platform.workspace","version":"1.0.0"},
        {"id":"platform.context","version":"1.0.0"},
        {"id":"platform.tasks","version":"1.0.0"}
    ]));
    s.set_workspace_root(&root.0).unwrap();
    let channels = s.channels(Some(&g)).unwrap();
    let c = channels["channels"][0]["id"].as_str().unwrap();
    s.subscribe_channel(
        Actor::Operator,
        c,
        &ChannelSubscription {
            instance_id: Some(a[1].clone()),
            subscribed: true,
        },
        "subscribe-reader",
    )
    .unwrap();
    s.reconcile_goals().unwrap();
    let runs = (0..3)
        .map(|_| s.claim().unwrap().unwrap())
        .collect::<Vec<_>>();
    let source = runs.iter().find(|r| r["instance_id"] == a[0]).unwrap();
    let run = source["id"].as_str().unwrap();
    let content =
        "Research source: supplied material. Finding: shared files survive the original Run.";
    s.workspace_write(
        run,
        &WriteInput {
            path: "research/source.md".into(),
            content: content.into(),
        },
        "write-evidence",
    )
    .unwrap();
    let file = s
        .publish_workspace_artifact(
            run,
            &PublishInput {
                name: "source evidence".into(),
                path: Some("research/source.md".into()),
                value: None,
            },
            "publish-evidence",
        )
        .unwrap();
    let file_id = file["id"].as_str().unwrap();
    s.post_channel(
        Actor::AgentRun(run),
        c,
        &ChannelPost {
            text: format!("Review published file {file_id}"),
            wake_subscribers: true,
        },
        "request-review",
    )
    .unwrap();
    for run in &runs {
        finish(&s, run);
    }
    s.reconcile_goals().unwrap();
    s.reconcile_channels().unwrap();
    s.reconcile_messages().unwrap();
    let wake = s.claim().unwrap().unwrap();
    let wake_run = wake["id"].as_str().unwrap();
    assert_eq!(wake["instance_id"], a[1]);
    let has_task: bool = s
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM work_items WHERE run_id=?)",
                [wake_run],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(!has_task);
    let tasks = s.task_board(wake_run).unwrap();
    assert_eq!(tasks["goal_id"], g);
    assert_eq!(tasks["can_request_tasks"], false);
    assert_eq!(tasks["tasks"].as_array().unwrap().len(), 3);
    assert_eq!(
        s.request_next_task(
            wake_run,
            &NextInput {
                task_key: "task-0".into(),
                expected_generation: 0,
            },
            "notification-cannot-claim",
        )
        .unwrap_err()
        .1,
        "autonomy_run_not_authorized"
    );
    let evidence = s.read_workspace_artifact(wake_run, file_id).unwrap();
    assert_eq!(evidence["content"], content);
    assert_eq!(evidence["source_state"], "completed");
    assert_eq!(s.run_context(wake_run).unwrap()["goal_id"], g);
    s.publish_context(
        Author::AgentRun(wake_run),
        None,
        &ContextInput {
            key: "peer-review".into(),
            expected_revision: 0,
            title: "Peer file reviewed".into(),
            body: format!("Read published peer snapshot {file_id}: {content}"),
            kind: "observation".into(),
            status: "active".into(),
            artifact_ids: vec![],
        },
        "review-evidence",
    )
    .unwrap();
    let board = s.run_context(wake_run).unwrap();
    assert_eq!(board["entries"][0]["source_run_id"], wake_run);
    assert_eq!(board["entries"][0]["source_instance_id"], a[1]);
    let reply = s
        .post_channel(
            Actor::AgentRun(wake_run),
            c,
            &ChannelPost {
                text: "Read peer file and published the review to shared context.".into(),
                wake_subscribers: true,
            },
            "review-reply",
        )
        .unwrap();
    assert_eq!(reply["notification_reply_suppressed"], true);
    assert_eq!(reply["notification_count"], 0);
    assert_eq!(s.recent_messages().unwrap().len(), 1);
    s.set_channel_members(
        c,
        &ChannelMembers {
            members: vec![a[0].clone(), a[2].clone()],
        },
        "remove-notification-reader",
    )
    .unwrap();
    assert_eq!(
        s.read_workspace_artifact(wake_run, file_id).unwrap_err().1,
        "workspace_artifact_out_of_scope"
    );
    assert_eq!(
        s.run_context(wake_run).unwrap_err().1,
        "context_run_not_authorized"
    );
}
#[test]
fn only_goal_participants_can_join_and_removed_subscriptions_are_not_woken() {
    let (s, g, a) = setup();
    let outsider = s
        .spawn(
            &Spawn {
                preset_name: "channel-team".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "outsider",
        )
        .unwrap()["instance_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        s.create_channel(
            &CreateChannel {
                goal_id: g.clone(),
                name: "bad".into(),
                description: "".into(),
                members: vec![outsider]
            },
            "bad"
        )
        .unwrap_err()
        .1,
        "channel_member_not_in_goal"
    );
    let c = create(&s, &g, &a, "team");
    s.subscribe_channel(
        Actor::Operator,
        &c,
        &ChannelSubscription {
            instance_id: Some(a[1].clone()),
            subscribed: true,
        },
        "sub",
    )
    .unwrap();
    s.post_channel(
        Actor::Operator,
        &c,
        &ChannelPost {
            text: "operator task update".into(),
            wake_subscribers: true,
        },
        "update",
    )
    .unwrap();
    s.set_channel_members(
        &c,
        &ChannelMembers {
            members: vec![a[0].clone()],
        },
        "remove",
    )
    .unwrap();
    s.reconcile_channels().unwrap();
    assert!(s.recent_messages().unwrap().is_empty());
    let view = s.channel(Actor::Operator, &c).unwrap();
    assert_eq!(view["members"].as_array().unwrap().len(), 1);
    assert_eq!(view["posts"][0]["pending"], 0);
}
#[test]
fn crash_between_message_admission_and_delivery_mark_deduplicates_on_restart() {
    let (s, g, a) = setup();
    let c = create(&s, &g, &a, "team");
    s.subscribe_channel(
        Actor::Operator,
        &c,
        &ChannelSubscription {
            instance_id: Some(a[1].clone()),
            subscribed: true,
        },
        "sub",
    )
    .unwrap();
    s.post_channel(
        Actor::Operator,
        &c,
        &ChannelPost {
            text: "durable finding".into(),
            wake_subscribers: true,
        },
        "update",
    )
    .unwrap();
    s.reconcile_channels().unwrap();
    assert_eq!(s.recent_messages().unwrap().len(), 1);
    s.transaction(|tx| {
        tx.execute(
            "UPDATE channel_notifications SET state='pending',message_id=NULL",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    s.recover_startup().unwrap();
    s.reconcile_channels().unwrap();
    assert_eq!(s.recent_messages().unwrap().len(), 1);
}

#[test]
fn goal_creation_gives_channel_capable_agents_a_shared_unsubscribed_team_channel() {
    let (s, g, a) = setup();
    let channels = s.channels(Some(&g)).unwrap();
    let team = channels["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "团队讨论")
        .unwrap();
    assert_eq!(team["members"].as_array().unwrap().len(), 3);
    for id in &a {
        let member = team["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["instance_id"] == *id)
            .unwrap();
        assert_eq!(member["subscribed"], false);
    }
    s.reconcile_goals().unwrap();
    let run = s.claim().unwrap().unwrap();
    let visible = s.run_channels(run["id"].as_str().unwrap()).unwrap();
    assert!(
        visible["channels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == team["id"])
    );
    let post = s
        .post_channel(
            Actor::AgentRun(run["id"].as_str().unwrap()),
            team["id"].as_str().unwrap(),
            &ChannelPost {
                text: "first shared finding without manual channel setup".into(),
                wake_subscribers: true,
            },
            "first",
        )
        .unwrap();
    assert_eq!(post["notification_count"], 0);
}
#[test]
fn default_team_filters_unbound_instances_and_does_not_restore_removed_members() {
    let (s, g, a) = setup();
    s.publish_definition(json!({"name":"plain-worker","version":"1","purpose":"plain","objective":"work","instructions":"inspect","protocol":"demo","plugins":[]})).unwrap();
    let plain = s
        .spawn(
            &Spawn {
                preset_name: "plain-worker".into(),
                preset_version: "1".into(),
                count: 1,
            },
            "plain",
        )
        .unwrap()["instance_ids"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let mixed = s
        .create_goal(
            &GoalPlan {
                verification: None,
                title: "mixed capabilities".into(),
                objective: "only channel capable agents join".into(),
                acceptance: "capability preserved".into(),
                tasks: vec![
                    TaskPlan {
                        key: "bound".into(),
                        instance_id: a[0].clone(),
                        instructions: "share evidence".into(),
                        ..Default::default()
                    },
                    TaskPlan {
                        key: "plain".into(),
                        instance_id: plain.clone(),
                        instructions: "work".into(),
                        ..Default::default()
                    },
                ],
            },
            "mixed",
        )
        .unwrap();
    let channels = s.channels(Some(mixed["id"].as_str().unwrap())).unwrap();
    let team = &channels["channels"][0];
    assert_eq!(team["members"].as_array().unwrap().len(), 1);
    assert_eq!(team["members"][0]["instance_id"], a[0]);
    let original = s.channels(Some(&g)).unwrap();
    let id = original["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "团队讨论")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    s.set_channel_members(
        id,
        &ChannelMembers {
            members: a[1..].to_vec(),
        },
        "remove-original",
    )
    .unwrap();
    s.recover_startup().unwrap();
    s.reconcile_fleets().unwrap();
    s.reconcile_goals().unwrap();
    assert!(
        !s.channel(Actor::Operator, id).unwrap()["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["instance_id"] == a[0])
    );
}
#[test]
fn rework_replacements_join_the_default_team_without_resurrecting_removed_members() {
    let (s, g, a) = setup();
    let initial = s.channels(Some(&g)).unwrap();
    let channel = initial["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "团队讨论")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    s.set_channel_members(
        channel,
        &ChannelMembers {
            members: a[1..].to_vec(),
        },
        "remove",
    )
    .unwrap();
    s.reconcile_goals().unwrap();
    for _ in 0..3 {
        let run = s.claim().unwrap().unwrap();
        finish(&s, &run);
    }
    s.reconcile_goals().unwrap();
    s.rework_goal(
        &g,
        &harness3_platform::artifacts::ReworkInput {
            expected_round: 1,
            note: "new team updates evidence".into(),
            assignments: vec![],
            task_keys: vec![],
        },
        "new-team",
    )
    .unwrap();
    s.reconcile_fleets().unwrap();
    s.reconcile_goals().unwrap();
    let updated = s.goal(&g).unwrap();
    let members = s.channel(Actor::Operator, channel).unwrap()["members"]
        .as_array()
        .unwrap()
        .clone();
    for task in updated["tasks"].as_array().unwrap() {
        assert!(!a.iter().any(|old| task["instance_id"] == *old));
        assert!(
            members
                .iter()
                .any(|m| m["instance_id"] == task["instance_id"] && m["subscribed"] == false)
        );
    }
    assert!(!members.iter().any(|m| m["instance_id"] == a[0]));
}
