#![allow(missing_docs)]

use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use rand::random_range;
use timesimp::{SignedDuration, Timesimp};
use tokio::time::sleep;

static SETUP: LazyLock<()> = LazyLock::new(|| {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();
});

#[derive(Debug, Default)]
struct ClientSimp {
    offset: Option<SignedDuration>,
    delay: Duration,
    jitter_percent: u8,
    server: Arc<ServerSimp>,
}

#[derive(Debug, Default)]
struct ServerSimp {
    offset: Option<SignedDuration>,
}

#[derive(Debug, thiserror::Error)]
#[error("Test error")]
struct TestError;

impl Timesimp for ClientSimp {
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
        let delay = (self.delay / 2).as_nanos() as f64;
        let jitter = random_range(0.0..=(self.jitter_percent as f64)) / 100.0;
        let delay = Duration::from_nanos((delay * (1.0 - jitter)) as u64);

        sleep(delay).await;
        let res = self.server.answer_client(request).await;
        sleep(delay).await;
        res
    }

    async fn sleep(duration: std::time::Duration) {
        tokio::time::sleep(duration).await;
    }
}

impl Timesimp for ServerSimp {
    type Err = TestError;

    async fn load_offset(&self) -> Result<Option<SignedDuration>, Self::Err> {
        Ok(self.offset)
    }

    async fn store_offset(&mut self, _offset: SignedDuration) -> Result<(), Self::Err> {
        unimplemented!()
    }

    async fn query_server(
        &self,
        _request: timesimp::Request,
    ) -> Result<timesimp::Response, Self::Err> {
        unimplemented!()
    }

    async fn sleep(duration: std::time::Duration) {
        tokio::time::sleep(duration).await;
    }
}

#[tokio::test]
async fn no_delay() {
    *SETUP;

    let server = Arc::new(ServerSimp::default());

    let mut client = ClientSimp {
        server,
        ..Default::default()
    };

    let offset = client
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap();
    assert!(
        offset.unwrap() > SignedDuration::from_millis(-5)
            && offset.unwrap() < SignedDuration::from_millis(5),
        "offset = {offset:?}"
    );
}

#[tokio::test]
async fn some_delay() {
    *SETUP;

    let server = Arc::new(ServerSimp::default());

    let mut client = ClientSimp {
        delay: Duration::from_millis(200),
        server,
        ..Default::default()
    };

    let offset = client
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap();
    assert!(
        offset.unwrap() > SignedDuration::from_millis(-5)
            && offset.unwrap() < SignedDuration::from_millis(5),
        "offset = {offset:?}"
    );
}

#[tokio::test]
async fn much_delay() {
    *SETUP;

    let server = Arc::new(ServerSimp::default());

    let mut client = ClientSimp {
        delay: Duration::from_millis(2000),
        jitter_percent: 30,
        server,
        ..Default::default()
    };

    let offset = client
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap();
    assert!(
        offset.unwrap() > SignedDuration::from_millis(-5)
            && offset.unwrap() < SignedDuration::from_millis(5),
        "offset = {offset:?}"
    );
}

#[tokio::test]
async fn server_offset_positive() {
    *SETUP;

    let server = Arc::new(ServerSimp {
        offset: Some(SignedDuration::from_secs(5)),
    });

    let mut client = ClientSimp {
        delay: Duration::from_millis(20),
        server,
        ..Default::default()
    };

    let offset = client
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap()
        .unwrap()
        - SignedDuration::from_secs(5);
    assert!(
        offset > SignedDuration::from_millis(-5) && offset < SignedDuration::from_millis(5),
        "offset - 5s = {offset:?}"
    );
}

#[tokio::test]
async fn server_offset_negative() {
    *SETUP;

    let server = Arc::new(ServerSimp {
        offset: Some(SignedDuration::from_secs(-5)),
    });

    let mut client = ClientSimp {
        delay: Duration::from_millis(20),
        server,
        ..Default::default()
    };

    let offset = client
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap()
        .unwrap()
        + SignedDuration::from_secs(5);
    assert!(
        offset > SignedDuration::from_millis(-5) && offset < SignedDuration::from_millis(5),
        "offset + 5s = {offset:?}"
    );
}

#[tokio::test]
async fn client_offset_positive() {
    *SETUP;

    let server = Arc::new(ServerSimp::default());

    let mut client = ClientSimp {
        offset: Some(SignedDuration::from_secs(5)),
        delay: Duration::from_millis(20),
        server,
        ..Default::default()
    };

    let offset = client
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap()
        .unwrap();
    assert!(
        offset > SignedDuration::from_millis(-5) && offset < SignedDuration::from_millis(5),
        "offset = {offset:?}"
    );
}

#[tokio::test]
async fn client_offset_negative() {
    *SETUP;

    let server = Arc::new(ServerSimp::default());

    let mut client = ClientSimp {
        offset: Some(SignedDuration::from_secs(-5)),
        delay: Duration::from_millis(20),
        server,
        ..Default::default()
    };

    let offset = client
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap()
        .unwrap();
    assert!(
        offset > SignedDuration::from_millis(-5) && offset < SignedDuration::from_millis(5),
        "offset = {offset:?}"
    );
}

#[tokio::test]
async fn low_jitter() {
    *SETUP;

    let server = Arc::new(ServerSimp {
        offset: Some(SignedDuration::from_secs(5)),
    });

    let mut client = ClientSimp {
        delay: Duration::from_millis(20),
        jitter_percent: 10,
        server,
        ..Default::default()
    };

    let offset = client
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap()
        .unwrap()
        - SignedDuration::from_secs(5);
    assert!(
        offset > SignedDuration::from_millis(-5) && offset < SignedDuration::from_millis(5),
        "offset - 5s = {offset:?}"
    );
}

#[tokio::test]
async fn mid_jitter() {
    *SETUP;

    let server = Arc::new(ServerSimp {
        offset: Some(SignedDuration::from_secs(5)),
    });

    let mut client = ClientSimp {
        delay: Duration::from_millis(20),
        jitter_percent: 50,
        server,
        ..Default::default()
    };

    let offset = client
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap()
        .unwrap()
        - SignedDuration::from_secs(5);
    assert!(
        offset > SignedDuration::from_millis(-5) && offset < SignedDuration::from_millis(5),
        "offset - 5s = {offset:?}"
    );
}

#[tokio::test]
async fn high_jitter() {
    *SETUP;

    let server = Arc::new(ServerSimp {
        offset: Some(SignedDuration::from_secs(5)),
    });

    let mut client = ClientSimp {
        delay: Duration::from_millis(20),
        jitter_percent: 80,
        server,
        ..Default::default()
    };

    let offset = client
        .attempt_sync(timesimp::Settings::default())
        .await
        .unwrap()
        .unwrap()
        - SignedDuration::from_secs(5);
    assert!(
        offset > SignedDuration::from_millis(-5) && offset < SignedDuration::from_millis(5),
        "offset - 5s = {offset:?}"
    );
}
