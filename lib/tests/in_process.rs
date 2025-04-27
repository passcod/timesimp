#![allow(missing_docs)]

use std::sync::LazyLock;

use timesimp::{SignedDuration, Timesimp};

static SETUP: LazyLock<()> = LazyLock::new(|| {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();
});

#[derive(Debug, Default)]
struct TestSimp {
    offset: Option<SignedDuration>,
}

#[derive(Debug, thiserror::Error)]
#[error("Test error")]
struct TestError;

impl Timesimp for TestSimp {
    type Err = TestError;

    async fn load_offset(&self) -> Result<Option<SignedDuration>, Self::Err> {
        Ok(self.offset)
    }

    async fn store_offset(&mut self, offset: SignedDuration) -> Result<(), Self::Err> {
        self.offset = Some(offset);
        Ok(())
    }

    async fn query_server(
        &self,
        request: timesimp::Request,
    ) -> Result<timesimp::Response, Self::Err> {
        self.answer_client(request).await
    }

    async fn sleep(duration: std::time::Duration) {
        tokio::time::sleep(duration).await;
    }
}

#[tokio::test]
async fn null_offset() {
    *SETUP;

    let mut simp = TestSimp::default();

    let offset = simp
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap();
    assert!(
        offset.unwrap() > SignedDuration::from_micros(-50)
            && offset.unwrap() < SignedDuration::from_micros(50),
        "offset = {offset:?}"
    );
}

#[tokio::test]
async fn zero_offset() {
    *SETUP;

    let mut simp = TestSimp {
        offset: Some(SignedDuration::from_micros(0)),
    };

    let offset = simp
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap();
    assert!(
        offset.unwrap() > SignedDuration::from_micros(-50)
            && offset.unwrap() < SignedDuration::from_micros(50),
        "offset = {offset:?}"
    );
}

#[tokio::test]
async fn negative_starting_offset() {
    *SETUP;

    let mut simp = TestSimp {
        offset: Some(SignedDuration::from_secs(-5)),
    };

    let offset = simp
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap()
        .unwrap()
        + SignedDuration::from_secs(5);
    assert!(
        offset > SignedDuration::from_micros(-50) && offset < SignedDuration::from_micros(50),
        "offset + 5s = {offset:?}"
    );
}

#[tokio::test]
async fn positive_starting_offset() {
    *SETUP;

    let mut simp = TestSimp {
        offset: Some(SignedDuration::from_secs(5)),
    };

    let offset = simp
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap()
        .unwrap()
        - SignedDuration::from_secs(5);
    assert!(
        offset > SignedDuration::from_micros(-50) && offset < SignedDuration::from_micros(50),
        "offset - 5s = {offset:?}"
    );
}
