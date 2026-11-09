//! Sync engine tests through the HTTP API.

use serde_json::{Value, json};

use crate::app_tests::TestApp;

fn h(ms: u64, node: &str) -> String {
    format!("{ms:013}-0000-{node}")
}

async fn shared_work(app: &TestApp, id: &str, title: &str) {
    sqlx::query("INSERT INTO works (id, title, created_at, seq) VALUES (?, ?, 0, 0)")
        .bind(id)
        .bind(title)
        .execute(&app.state.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO work_people (work_id, position, name, role) VALUES (?, 0, 'Ursula K. Le Guin', 'author')")
        .bind(id)
        .execute(&app.state.db)
        .await
        .unwrap();
}

async fn sync(app: &TestApp, cookie: &str, since: i64, ops: Value) -> Value {
    let (status, body) = app
        .post(
            "/api/sync",
            json!({ "since": since, "ops": ops }),
            Some(cookie),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    body
}

fn op(id: &str, hlc: &str, op: Value) -> Value {
    json!({ "id": id, "hlc": hlc, "op": op })
}

/// A user with one book (`b1`) and a reading (`r1`, 100 pages) created at T0.
async fn setup() -> (TestApp, String) {
    let app = TestApp::new().await;
    shared_work(&app, "w1", "The Left Hand of Darkness").await;
    let (_, cookie) = app.user("rri", false).await;
    let t0 = h(1000, "aaa");
    let res = sync(
        &app,
        &cookie,
        0,
        json!([
            op("o1", &t0, json!({"type": "book.create", "id": "b1", "work_id": "w1", "rank": "V", "added_at": 1})),
            op("o2", &t0, json!({"type": "reading.create", "id": "r1", "book_id": "b1", "started_on": "2026-11-01", "total": 100})),
        ]),
    )
    .await;
    assert_eq!(res["dropped"], json!([]));
    assert_eq!(
        res["changes"]["works"][0]["title"],
        "The Left Hand of Darkness"
    );
    (app, cookie)
}

fn tap(id: &str, hlc: &str, base: &str, from: i64, to: i64) -> Value {
    op(
        id,
        hlc,
        json!({"type": "progress", "event_id": format!("e-{id}"), "reading_id": "r1", "base": base, "from": from, "to": to, "local_date": "2026-11-01"}),
    )
}

async fn reading_state(app: &TestApp, cookie: &str) -> (i64, Vec<String>) {
    let res = sync(app, cookie, 0, json!([])).await;
    let pos = res["changes"]["readings"][0]["pos"].as_i64().unwrap();
    let mut live: Vec<String> = res["changes"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["deleted"] == false)
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();
    live.sort();
    (pos, live)
}

#[tokio::test]
async fn later_offline_session_wins_and_the_other_stops_counting() {
    let (app, cookie) = setup().await;
    let t0 = h(1000, "aaa");
    let (a1, a2, b1, b2) = (
        h(2000, "aaa"),
        h(3000, "aaa"),
        h(2500, "bbb"),
        h(4000, "bbb"),
    );

    // Device A syncs two taps.
    sync(
        &app,
        &cookie,
        0,
        json!([tap("a1", &a1, &t0, 0, 1), tap("a2", &a2, &a1, 1, 2)]),
    )
    .await;
    // Device B was offline from T0 with three taps; its last tap is newest.
    let res = sync(
        &app,
        &cookie,
        0,
        json!([
            tap("b1", &b1, &t0, 0, 1),
            tap("b2", &b2, &b1, 1, 2),
            tap("b3", &h(4100, "bbb"), &b2, 2, 3)
        ]),
    )
    .await;
    assert_eq!(res["dropped"], json!([]));
    let (pos, live) = reading_state(&app, &cookie).await;
    assert_eq!(pos, 3);
    assert_eq!(live, vec!["e-b1", "e-b2", "e-b3"]);
}

#[tokio::test]
async fn older_offline_session_loses_entirely() {
    let (app, cookie) = setup().await;
    let t0 = h(1000, "aaa");
    sync(
        &app,
        &cookie,
        0,
        json!([tap("b1", &h(5000, "bbb"), &t0, 0, 5)]),
    )
    .await;
    let res = sync(
        &app,
        &cookie,
        0,
        json!([
            tap("a1", &h(2000, "aaa"), &t0, 0, 1),
            tap("a2", &h(3000, "aaa"), &h(2000, "aaa"), 1, 2)
        ]),
    )
    .await;
    assert_eq!(res["dropped"].as_array().unwrap().len(), 2);
    let (pos, live) = reading_state(&app, &cookie).await;
    assert_eq!(pos, 5);
    assert_eq!(live, vec!["e-b1"]);
}

#[tokio::test]
async fn undo_removes_the_event_and_restores_the_position() {
    let (app, cookie) = setup().await;
    let (t0, a1, a2) = (h(1000, "aaa"), h(2000, "aaa"), h(3000, "aaa"));
    sync(&app, &cookie, 0, json!([tap("a1", &a1, &t0, 0, 1)])).await;
    sync(
        &app,
        &cookie,
        0,
        json!([op("u1", &a2, json!({"type": "progress.undo", "reading_id": "r1", "event_id": "e-a1", "base": a1, "to": 0}))]),
    )
    .await;
    let (pos, live) = reading_state(&app, &cookie).await;
    assert_eq!(pos, 0);
    assert!(live.is_empty());
}

#[tokio::test]
async fn fields_are_last_writer_wins() {
    let (app, cookie) = setup().await;
    let set = |id: &str, hlc: &str, fields: Value| {
        op(
            id,
            hlc,
            json!({"type": "book.set", "id": "b1", "fields": fields}),
        )
    };
    sync(
        &app,
        &cookie,
        0,
        json!([set(
            "s1",
            &h(3000, "aaa"),
            json!({"rating": 4, "notes": "new"})
        )]),
    )
    .await;
    // An older change to rating loses; its change to rank (untouched since) wins.
    let res = sync(
        &app,
        &cookie,
        0,
        json!([set(
            "s2",
            &h(2000, "bbb"),
            json!({"rating": 2, "rank": "k"})
        )]),
    )
    .await;
    let book = &res["changes"]["books"][0];
    assert_eq!(book["rating"], 4);
    assert_eq!(book["notes"], "new");
    assert_eq!(book["rank"], "k");
}

#[tokio::test]
async fn deleting_a_book_tombstones_everything_and_later_ops_drop() {
    let (app, cookie) = setup().await;
    let res = sync(
        &app,
        &cookie,
        0,
        json!([
            op(
                "t1",
                &h(2000, "aaa"),
                json!({"type": "tag.set", "book_id": "b1", "tag": "sci-fi", "present": true})
            ),
            op(
                "d1",
                &h(3000, "aaa"),
                json!({"type": "book.delete", "id": "b1"})
            ),
            op(
                "t2",
                &h(4000, "aaa"),
                json!({"type": "tag.set", "book_id": "b1", "tag": "late", "present": true})
            ),
        ]),
    )
    .await;
    assert_eq!(res["dropped"], json!(["t2"]));
    let ch = &res["changes"];
    assert_eq!(ch["books"][0]["deleted"], true);
    assert_eq!(ch["readings"][0]["deleted"], true);
    assert_eq!(ch["tags"][0]["present"], false);
}

#[tokio::test]
async fn pull_returns_only_newer_changes() {
    let (app, cookie) = setup().await;
    let first = sync(&app, &cookie, 0, json!([])).await;
    let since = first["changes"]["seq"].as_i64().unwrap();
    let again = sync(&app, &cookie, since, json!([])).await;
    assert_eq!(again["changes"]["books"], json!([]));
    let after = sync(
        &app,
        &cookie,
        since,
        json!([op(
            "s",
            &h(9000, "aaa"),
            json!({"type": "book.set", "id": "b1", "fields": {"active": true}})
        )]),
    )
    .await;
    assert_eq!(after["changes"]["books"][0]["active"], true);
    assert!(after["changes"]["readings"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn other_users_rows_are_untouchable_and_invalid_ops_are_rejected() {
    let (app, _) = setup().await;
    let (_, other) = app.user("mallory", false).await;
    let res = sync(
        &app,
        &other,
        0,
        json!([
            op(
                "x1",
                &h(9000, "mmm"),
                json!({"type": "book.set", "id": "b1", "fields": {"notes": "mine"}})
            ),
            op(
                "x2",
                &h(9000, "mmm"),
                json!({"type": "book.set", "id": "b1", "fields": {"rating": 9}})
            ),
        ]),
    )
    .await;
    assert_eq!(res["dropped"], json!(["x1", "x2"]));
    assert!(res["changes"]["books"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn invalid_ops_are_rejected_without_partial_changes() {
    let (app, cookie) = setup().await;
    let res = sync(
        &app,
        &cookie,
        0,
        json!([op(
            "bad",
            &h(9000, "aaa"),
            json!({"type": "book.set", "id": "b1", "fields": {"notes": "kept?", "rating": 9}})
        )]),
    )
    .await;
    assert_eq!(res["rejected"][0]["id"], "bad");
    assert_eq!(res["changes"]["books"][0]["notes"], "");
}

#[tokio::test]
async fn private_manual_works_are_visible_only_to_their_owner() {
    let app = TestApp::new().await;
    let (_, cookie) = app.user("rri", false).await;
    let res = sync(
        &app,
        &cookie,
        0,
        json!([
            op("w", &h(1000, "aaa"), json!({"type": "work.create", "work": {"id": "pw", "title": "Zine Collection", "first_year": 2025, "people": [{"name": "Various", "role": "author"}], "edition": {"id": "pe", "publisher": "Self", "binding": "Paperback", "year": 2025, "isbn13": null, "pages": 40}}})),
            op("b", &h(1000, "aaa"), json!({"type": "book.create", "id": "pb", "work_id": "pw", "rank": "V", "added_at": 1})),
        ]),
    )
    .await;
    assert_eq!(res["dropped"], json!([]), "{res}");
    assert_eq!(res["changes"]["works"][0]["private"], true);
    assert_eq!(res["changes"]["works"][0]["editions"][0]["pages"], 40);

    let (_, other) = app.user("juno", false).await;
    let res = sync(
        &app,
        &other,
        0,
        json!([op(
            "b2",
            &h(2000, "jjj"),
            json!({"type": "book.create", "id": "jb", "work_id": "pw", "rank": "V", "added_at": 1})
        )]),
    )
    .await;
    assert_eq!(res["dropped"], json!(["b2"]));
}

#[tokio::test]
async fn odd_ids_and_cover_names_are_rejected() {
    let (app, cookie) = setup().await;
    let t1 = h(2000, "aaa");
    let long = "x".repeat(65);
    let res = sync(
        &app,
        &cookie,
        0,
        json!([
            op("o3", &t1, json!({"type": "book.create", "id": "../b2", "work_id": "w1", "rank": "W", "added_at": 2})),
            op("o4", &t1, json!({"type": "book.create", "id": long, "work_id": "w1", "rank": "W", "added_at": 2})),
            op("o5", &t1, json!({"type": "book.set", "id": "b1", "fields": {"cover_override": "../../api/export.json?"}})),
            op("o6", &t1, json!({"type": "book.set", "id": "b1", "fields": {"cover_override": "0123456789abcdef0123456789abcdef"}})),
        ]),
    )
    .await;
    let rejected: Vec<&str> = res["rejected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(rejected, ["o3", "o4", "o5"], "{res}");
    let book = res["changes"]["books"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == "b1")
        .unwrap();
    assert_eq!(book["cover_override"], "0123456789abcdef0123456789abcdef");
}
