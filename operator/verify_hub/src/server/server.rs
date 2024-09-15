use axum::http::Method;
use axum::routing::get;
use std::time::Duration;
use axum::error_handling::HandleErrorLayer;
use axum::{routing::post, Router};
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::PgConnection;
use dotenvy::dotenv;
use ed25519_dalek::{Signature, Signer, SigningKey};
use rand::rngs::OsRng;
use reqwest::Client;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;


use crate::config::Config;
use crate::opml::handler::*;
use crate::opml::model::{OpmlAnswer, OpmlRequest};
use crate::tee::handler::*;
use crate::tee::model::{AnswerReq, Operator, OperatorReq, OperatorResp, Params, WorkerStatus};

#[derive(Debug, Clone)]
pub struct Server {
    pub sign_key: SigningKey,
    pub tee_operator_collections: HashMap<String, Operator>,
    //pub pg: Pool<ConnectionManager<PgConnection>>,
    pub tee_channels: HashMap<String, mpsc::Sender<AnswerReq>>,
    pub opml_channels: HashMap<String, mpsc::Sender<OpmlAnswer>>,
}

#[derive(Debug, Clone)]
pub struct SharedState(pub(crate) Arc<RwLock<Server>>);

impl SharedState {
    pub async fn new(config: Config) -> Self {
        let server = Server::new(config).await;
        SharedState(Arc::new(RwLock::new(server)))
    }
}

impl Server {
    pub async fn new(config: Config) -> Self {
        let mut csprng = OsRng;
        let sign_key = SigningKey::generate(&mut csprng);
        dotenv().ok();

        //let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        //let manager = ConnectionManager::<PgConnection>::new(database_url);
        //let pg = Pool::builder() .build(manager) .expect("Failed to create pool.");

        Self {
            sign_key,
            tee_operator_collections: Default::default(),
            //pg,
            tee_channels: Default::default(),
            opml_channels: Default::default(),
        }
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        self.sign_key.sign(message)
    }

    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        self.sign_key.verify(message, signature).is_ok()
    }

    pub fn add_worker(
        &mut self,
        worker_name: String,
        check_heart_beat: bool,
        worker_status: WorkerStatus,
        multimodal: bool,
    ) {
        let worker_name_clone = worker_name.clone();
        let operator = Operator {
            worker_name: worker_name_clone,
            check_heart_beat,
            worker_status,
            multimodal,
        };
        self.tee_operator_collections.insert(worker_name, operator);
    }

    pub async fn send_tee_inductive_task(
        &self,
        worker_name: String,
        req: OperatorReq,
    ) -> OperatorResp {
        let operator = self.tee_operator_collections.get(&worker_name).unwrap();
        let op_url = format!("{}/api/v1/question", operator.worker_name);
        //let client = Client::builder().proxy(reqwest::Proxy::http("http://127.0.0.1:8080")?).build().unwrap();
        let resp = Client::new().post(op_url).json(&req).send().await.unwrap();

        resp.json::<OperatorResp>().await.unwrap()
    }

    pub async fn send_opml_request(
        &self,
        req: OpmlRequest,
    ) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Sending opml request {:?}", req);
        let client = reqwest::Client::new();
        let opml_server_url = format!("{}/api/v1/question", "http://127.0.0.1:1234");
        tracing::info!("{:?}", opml_server_url);

        let response = client.post(opml_server_url).json(&req).send().await?;
        tracing::info!("{:?}", response);

        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("OPML server responded with status: {}", response.status()).into())
        }
    }
}

pub async fn run(url: &str, tx: tokio::sync::oneshot::Sender<SharedState>) {

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    let config = Config::new();
    let server = SharedState::new(config).await;
    let server_clone = server.clone();

    // build our application with a single route
    let app = Router::new()
        .route("/ping", get(|| async { tracing::info!("<-- ping");"pong" }))
        .route("/reister_worker", post(register_worker))
        .route("/receive_heart_beat", post(receive_heart_beat))
        .route("/api/tee_callback", post(tee_callback))
        .route("/api/opml_callback", post(opml_callback))
        .layer(cors)
        .layer(
            ServiceBuilder::new()
            .layer(HandleErrorLayer::new(handle_error))
            .timeout(Duration::from_secs(600))
            .layer(TraceLayer::new_for_http())
        )
        .with_state(server);

    tx.send(server_clone).unwrap();
    //let listener = tokio::net::TcpListener::bind("0.0.0.0:21001").await.unwrap();
    let listener = tokio::net::TcpListener::bind(url).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
