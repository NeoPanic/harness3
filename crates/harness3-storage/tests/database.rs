use harness3_storage::{Database, error::fault};
#[test]
fn schema_and_transaction_failures_preserve_atomicity() {
    let db = Database::open(std::path::Path::new(":memory:")).unwrap();
    let tables: i64 = db
        .lock()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 49);
    let result: Result<(), _> = db.transaction(|tx| {
        tx.execute(
            "INSERT INTO events(kind,subject,body,at) VALUES('test','test','{}',0)",
            [],
        )?;
        Err(fault(409, "test_rollback"))
    });
    assert!(result.is_err());
    assert_eq!(
        db.lock()
            .unwrap()
            .query_row("SELECT count(*) FROM events", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.lock()
            .unwrap()
            .query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
